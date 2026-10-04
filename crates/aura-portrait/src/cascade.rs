//! A Viola-Jones cascade evaluator, faithful to OpenCV's, over OpenCV's own trained cascades.
//!
//! # Why a cascade and not a network
//!
//! `aura-infer` interprets a documented ONNX subset without `Resize` or `ConvTranspose`, and
//! the face models it ships are placeholders (phase 06 condition C1). A boosted cascade needs
//! neither: it is integral images, rectangle sums and a few thousand comparisons, and its
//! trained weights are published under OpenCV's BSD-style Intel licence as plain text. It is
//! the first detector in this product that finds a face in a photograph somebody actually took.
//!
//! The evaluation follows OpenCV 4's `CascadeClassifier` step for step, because the
//! thresholds in the cascade files were learned against that exact arithmetic:
//!
//! * the image is **resized** for each scale and the 20x20 window is fixed - the features
//!   are never scaled;
//! * each window is normalised by the standard deviation of its inner 18x18, and a window
//!   whose deviation is below ten grey levels is rejected outright (flat walls and skies);
//! * trees descend `left` when the normalised feature is below the node threshold, and a
//!   non-positive child index is a leaf;
//! * a stage passes when the sum of its leaves reaches its threshold less `1e-5`;
//! * the scan steps two pixels below a scale factor of two and one above it, and skips a
//!   position after a window fails the very first stage;
//! * detections are grouped with OpenCV's `groupRectangles` - an equivalence partition by
//!   `SimilarRects(0.2)`, an average per class, a neighbour threshold, and the removal of a
//!   small class inside a large one.
//!
//! The one deliberate difference is the resampler, which is bilinear with centre alignment as
//! OpenCV's is but not bit-identical to it, so a borderline window may land differently.
//! `tests/local_eval.rs` compares the evaluator with boxes OpenCV itself produced on the same
//! grey frames; on the day this shipped all 80 boxes on 22 photographs agreed to within two
//! pixels and two neighbours.
//!
//! # Determinism
//!
//! Every scale is evaluated independently and the candidate lists are concatenated in scale
//! order, so the parallel scan returns the same list on every machine - invariant 4.

use std::sync::OnceLock;

use rayon::prelude::*;

/// OpenCV's `THRESHOLD_EPS`, subtracted from every stage threshold at load.
const THRESHOLD_EPS: f32 = 1e-5;

/// The frontal face cascade: Rainer Lienhart's tree-based 20x20 gentle AdaBoost.
const FRONTAL_TEXT: &str = include_str!("../data/frontalface_alt2.cascade");
/// The profile cascade, which finds faces turned to one side; the other side is a mirror.
const PROFILE_TEXT: &str = include_str!("../data/profileface.cascade");
/// The eye cascade, run only inside the upper half of a face that was already found.
const EYE_TEXT: &str = include_str!("../data/eye.cascade");

/// One weighted rectangle of a Haar feature, in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    weight: f32,
}

/// A Haar feature: two or three weighted rectangles. Fixed-size, so the hot loop touches no
/// heap pointer per feature.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Feature {
    rects: [Rect; 3],
    count: usize,
}

/// One split of a tree.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Node {
    left: i32,
    right: i32,
    feature: usize,
    threshold: f32,
}

/// One weak classifier, as offsets into the cascade's flat node and leaf arrays.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tree {
    first_node: usize,
    node_count: usize,
    first_leaf: usize,
}

/// One stage of the cascade: a threshold and a run of trees.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stage {
    threshold: f32,
    first_tree: usize,
    tree_count: usize,
}

/// A trained cascade, flattened: every stage, tree, node and leaf in one array each.
#[derive(Debug, Clone, PartialEq)]
pub struct Cascade {
    width: u32,
    height: u32,
    stages: Vec<Stage>,
    trees: Vec<Tree>,
    nodes: Vec<Node>,
    leaves: Vec<f32>,
    features: Vec<Feature>,
}

/// One node with its feature's corners resolved against one integral image's stride: four
/// offsets per rectangle, so a rectangle sum is four loads relative to the window origin, and
/// the evaluation streams through one array instead of chasing a feature index per node.
#[derive(Debug, Clone, Copy)]
struct Placed {
    offsets: [[u32; 4]; 3],
    weights: [f32; 3],
    count: u8,
    threshold: f32,
    left: i32,
    right: i32,
}

/// Why a cascade file did not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadeError(pub String);

impl std::fmt::Display for CascadeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "cascade did not parse: {}", self.0)
    }
}

impl std::error::Error for CascadeError {}

/// One grouped detection, in pixels of the image it was found in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
    /// How many raw windows agreed. The cascade's own measure of how sure it is.
    pub neighbours: u32,
}

impl Detection {
    /// Centre, x.
    #[must_use]
    pub fn cx(&self) -> f32 {
        self.x + self.w * 0.5
    }

    /// Centre, y.
    #[must_use]
    pub fn cy(&self) -> f32 {
        self.y + self.h * 0.5
    }

    /// Intersection over union with another detection.
    #[must_use]
    pub fn iou(&self, other: &Self) -> f32 {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.w).min(other.x + other.w);
        let y1 = (self.y + self.h).min(other.y + other.h);
        let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
        let union = self.w * self.h + other.w * other.h - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }
}

/// How to scan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanParams {
    /// Growth of the window between scales. OpenCV's default is `1.1`.
    pub scale_factor: f32,
    /// Raw windows a class needs *more than* to survive grouping. OpenCV's default is 3.
    pub min_neighbours: u32,
    /// Smallest window, in pixels of the scanned image.
    pub min_size: u32,
    /// Largest window, in pixels of the scanned image. Zero means the whole image.
    pub max_size: u32,
}

impl Default for ScanParams {
    fn default() -> Self {
        Self {
            scale_factor: 1.1,
            min_neighbours: 3,
            min_size: 20,
            max_size: 0,
        }
    }
}

/// An 8-bit grey image.
#[derive(Debug, Clone, PartialEq)]
pub struct GreyImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major luma.
    pub pixels: Vec<u8>,
}

impl GreyImage {
    /// Wrap a buffer. `None` when it is shorter than the size it claims.
    #[must_use]
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        ((width as usize) * (height as usize) <= pixels.len()).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// The grey value at a pixel, clamped into the image.
    #[must_use]
    pub fn value(&self, x: i64, y: i64) -> f32 {
        let cx = x.clamp(0, i64::from(self.width.max(1)) - 1) as usize;
        let cy = y.clamp(0, i64::from(self.height.max(1)) - 1) as usize;
        f32::from(
            self.pixels
                .get(cy * self.width as usize + cx)
                .copied()
                .unwrap_or(0),
        )
    }

    /// Bilinear resize, centre-aligned, rounded back to bytes - OpenCV's `INTER_LINEAR`.
    ///
    /// The column taps are computed once per call rather than once per pixel, which is most of
    /// the cost of a scan's twenty-five resizes.
    #[must_use]
    pub fn resize(&self, width: u32, height: u32) -> Self {
        if width == self.width && height == self.height {
            return self.clone();
        }
        let src_w = self.width.max(1) as usize;
        let src_h = self.height.max(1) as usize;
        let sx = self.width as f32 / width.max(1) as f32;
        let sy = self.height as f32 / height.max(1) as f32;
        let taps = |out: u32, scale: f32, limit: usize| -> Vec<(usize, usize, f32)> {
            (0..out)
                .map(|i| {
                    let f = ((i as f32 + 0.5) * scale - 0.5).max(0.0);
                    let i0 = (f.floor() as usize).min(limit - 1);
                    let i1 = (i0 + 1).min(limit - 1);
                    (i0, i1, f - f.floor())
                })
                .collect()
        };
        let columns = taps(width, sx, src_w);
        let rows = taps(height, sy, src_h);
        let mut pixels = Vec::with_capacity((width as usize) * (height as usize));
        for (y0, y1, ty) in rows {
            let top = self
                .pixels
                .get(y0 * src_w..(y0 + 1) * src_w)
                .unwrap_or_default();
            let bottom = self
                .pixels
                .get(y1 * src_w..(y1 + 1) * src_w)
                .unwrap_or_default();
            for (x0, x1, tx) in &columns {
                let p = |row: &[u8], i: usize| f32::from(row.get(i).copied().unwrap_or(0));
                let t = p(top, *x0) * (1.0 - tx) + p(top, *x1) * tx;
                let b = p(bottom, *x0) * (1.0 - tx) + p(bottom, *x1) * tx;
                pixels.push((t * (1.0 - ty) + b * ty + 0.5).clamp(0.0, 255.0) as u8);
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// A sub-rectangle, clamped into the image.
    #[must_use]
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Self {
        let x0 = x.min(self.width);
        let y0 = y.min(self.height);
        let w = width.min(self.width - x0);
        let h = height.min(self.height - y0);
        let mut pixels = Vec::with_capacity((w as usize) * (h as usize));
        for yy in y0..y0 + h {
            let start = (yy as usize) * (self.width as usize) + x0 as usize;
            pixels.extend(
                self.pixels
                    .get(start..start + w as usize)
                    .unwrap_or_default()
                    .iter(),
            );
        }
        Self {
            width: w,
            height: h,
            pixels,
        }
    }

    /// The image mirrored left to right.
    #[must_use]
    pub fn mirrored(&self) -> Self {
        let mut pixels = Vec::with_capacity(self.pixels.len());
        for y in 0..self.height as usize {
            for x in (0..self.width as usize).rev() {
                pixels.push(
                    self.pixels
                        .get(y * self.width as usize + x)
                        .copied()
                        .unwrap_or(0),
                );
            }
        }
        Self {
            width: self.width,
            height: self.height,
            pixels,
        }
    }
}

/// Summed-area tables of an image and of its square.
struct Integral {
    stride: usize,
    sum: Vec<i32>,
    sq: Vec<i64>,
}

impl Integral {
    fn new(image: &GreyImage) -> Self {
        let w = image.width as usize;
        let h = image.height as usize;
        let stride = w + 1;
        let mut sum = vec![0_i32; stride * (h + 1)];
        let mut sq = vec![0_i64; stride * (h + 1)];
        for y in 0..h {
            let mut row = 0_i32;
            let mut row_sq = 0_i64;
            for x in 0..w {
                let byte = image.pixels.get(y * w + x).copied().unwrap_or(0);
                let v = i32::from(byte);
                row += v;
                row_sq += i64::from(v) * i64::from(v);
                let above = sum.get(y * stride + x + 1).copied().unwrap_or(0);
                let above_sq = sq.get(y * stride + x + 1).copied().unwrap_or(0);
                if let Some(slot) = sum.get_mut((y + 1) * stride + x + 1) {
                    *slot = above + row;
                }
                if let Some(slot) = sq.get_mut((y + 1) * stride + x + 1) {
                    *slot = above_sq + row_sq;
                }
            }
        }
        Self { stride, sum, sq }
    }

    #[inline]
    fn rect<T>(table: &[T], stride: usize, x: usize, y: usize, w: usize, h: usize) -> i64
    where
        T: Copy + Default + Into<i64>,
    {
        let at = |i: usize| -> i64 { table.get(i).copied().unwrap_or_default().into() };
        at((y + h) * stride + x + w) - at(y * stride + x + w) - at((y + h) * stride + x)
            + at(y * stride + x)
    }
}

impl Cascade {
    /// Parse the line format `data/convert_opencv_cascade.py` writes.
    ///
    /// # Errors
    ///
    /// [`CascadeError`] naming the line that did not parse.
    pub fn parse(text: &str) -> Result<Self, CascadeError> {
        let mut lines = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'));
        let header = lines
            .next()
            .ok_or_else(|| CascadeError("empty file".to_string()))?;
        let head: Vec<&str> = header.split_whitespace().collect();
        if head.first() != Some(&"cascade") || head.len() != 5 {
            return Err(CascadeError(format!("bad header `{header}`")));
        }
        let width = number::<u32>(head.get(1))?;
        let height = number::<u32>(head.get(2))?;
        let stage_count = number::<usize>(head.get(3))?;
        let feature_count = number::<usize>(head.get(4))?;

        let mut stages = Vec::with_capacity(stage_count);
        let mut trees = Vec::new();
        let mut nodes = Vec::new();
        let mut leaves = Vec::new();
        for _ in 0..stage_count {
            let line = lines
                .next()
                .ok_or_else(|| CascadeError("missing stage".to_string()))?;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.first() != Some(&"s") {
                return Err(CascadeError(format!("expected a stage, found `{line}`")));
            }
            let threshold = number::<f32>(parts.get(1))? - THRESHOLD_EPS;
            let tree_count = number::<usize>(parts.get(2))?;
            let first_tree = trees.len();
            for _ in 0..tree_count {
                let line = lines
                    .next()
                    .ok_or_else(|| CascadeError("missing tree".to_string()))?;
                let (tree_nodes, tree_leaves) = parse_tree(line)?;
                trees.push(Tree {
                    first_node: nodes.len(),
                    node_count: tree_nodes.len(),
                    first_leaf: leaves.len(),
                });
                nodes.extend(tree_nodes);
                leaves.extend(tree_leaves);
            }
            stages.push(Stage {
                threshold,
                first_tree,
                tree_count,
            });
        }

        let mut features = Vec::with_capacity(feature_count);
        for _ in 0..feature_count {
            let line = lines
                .next()
                .ok_or_else(|| CascadeError("missing feature".to_string()))?;
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.first() != Some(&"f") {
                return Err(CascadeError(format!("expected a feature, found `{line}`")));
            }
            let count = number::<usize>(parts.get(1))?;
            if !(1..=3).contains(&count) {
                return Err(CascadeError(format!("a feature has {count} rectangles")));
            }
            let mut rects = [Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                weight: 0.0,
            }; 3];
            for (r, slot) in rects.iter_mut().enumerate().take(count) {
                let at = 2 + r * 5;
                let rect = Rect {
                    x: number(parts.get(at))?,
                    y: number(parts.get(at + 1))?,
                    w: number(parts.get(at + 2))?,
                    h: number(parts.get(at + 3))?,
                    weight: number(parts.get(at + 4))?,
                };
                if rect.x + rect.w > width || rect.y + rect.h > height {
                    return Err(CascadeError(format!(
                        "rectangle outside the window: `{line}`"
                    )));
                }
                *slot = rect;
            }
            features.push(Feature { rects, count });
        }

        if nodes.iter().any(|n: &Node| n.feature >= features.len()) {
            return Err(CascadeError("a node names a missing feature".to_string()));
        }

        Ok(Self {
            width,
            height,
            stages,
            trees,
            nodes,
            leaves,
            features,
        })
    }

    /// The shipped frontal face cascade, parsed once.
    #[must_use]
    pub fn frontal() -> Option<&'static Self> {
        static CELL: OnceLock<Option<Cascade>> = OnceLock::new();
        CELL.get_or_init(|| Self::parse(FRONTAL_TEXT).ok()).as_ref()
    }

    /// The shipped profile cascade, parsed once.
    #[must_use]
    pub fn profile() -> Option<&'static Self> {
        static CELL: OnceLock<Option<Cascade>> = OnceLock::new();
        CELL.get_or_init(|| Self::parse(PROFILE_TEXT).ok()).as_ref()
    }

    /// The shipped eye cascade, parsed once.
    #[must_use]
    pub fn eye() -> Option<&'static Self> {
        static CELL: OnceLock<Option<Cascade>> = OnceLock::new();
        CELL.get_or_init(|| Self::parse(EYE_TEXT).ok()).as_ref()
    }

    /// The window the cascade was trained on.
    #[must_use]
    pub fn window(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Number of stages.
    #[must_use]
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Scan an image at every scale and return the grouped detections.
    #[must_use]
    pub fn detect(&self, image: &GreyImage, params: ScanParams) -> Vec<Detection> {
        let raw = self.detect_raw(image, params);
        group_rectangles(&raw, params.min_neighbours, 0.2)
    }

    /// Every window that passed every stage, before grouping.
    #[must_use]
    pub fn detect_raw(&self, image: &GreyImage, params: ScanParams) -> Vec<[i32; 4]> {
        let factor_step = params.scale_factor.max(1.01);
        let max_w = if params.max_size == 0 {
            image.width
        } else {
            params.max_size.min(image.width)
        };
        let max_h = if params.max_size == 0 {
            image.height
        } else {
            params.max_size.min(image.height)
        };
        let mut scales = Vec::new();
        let mut factor = 1.0_f64;
        loop {
            let win_w = (f64::from(self.width) * factor).round() as u32;
            let win_h = (f64::from(self.height) * factor).round() as u32;
            if win_w > max_w || win_h > max_h {
                break;
            }
            let scaled_w = (f64::from(image.width) / factor).round() as u32;
            let scaled_h = (f64::from(image.height) / factor).round() as u32;
            if scaled_w < self.width || scaled_h < self.height {
                break;
            }
            if win_w >= params.min_size && win_h >= params.min_size {
                scales.push(factor);
            }
            factor *= f64::from(factor_step);
        }

        let per_scale: Vec<Vec<[i32; 4]>> = scales
            .par_iter()
            .map(|factor| self.scan_scale(image, *factor))
            .collect();
        per_scale.into_iter().flatten().collect()
    }

    fn scan_scale(&self, image: &GreyImage, factor: f64) -> Vec<[i32; 4]> {
        let scaled_w = (f64::from(image.width) / factor).round() as u32;
        let scaled_h = (f64::from(image.height) / factor).round() as u32;
        let scaled = image.resize(scaled_w, scaled_h);
        let integral = Integral::new(&scaled);
        let (placed, reach) = self.place(integral.stride);
        let win_w = (f64::from(self.width) * factor).round() as i32;
        let win_h = (f64::from(self.height) * factor).round() as i32;
        let step = if factor >= 2.0 { 1 } else { 2 };
        // OpenCV's working size: the integral image (one wider than the image) less the window,
        // so the last window sits flush with the right and bottom edges.
        let span_x = (scaled_w + 1).saturating_sub(self.width) as usize;
        let span_y = (scaled_h + 1).saturating_sub(self.height) as usize;
        let mut found = Vec::new();
        let mut y = 0;
        while y < span_y {
            let mut x = 0;
            while x < span_x {
                let result = self.run_at(&integral, &placed, reach, x, y);
                if result > 0 {
                    found.push([
                        (x as f64 * factor).round() as i32,
                        (y as f64 * factor).round() as i32,
                        win_w,
                        win_h,
                    ]);
                }
                if result == 0 {
                    x += step;
                }
                x += step;
            }
            y += step;
        }
        found
    }

    /// Resolve every node's feature against an integral image's stride.
    ///
    /// Returns the placed nodes and the furthest offset any of them reads, so the scan can
    /// check once per window that the whole window is inside the table.
    fn place(&self, stride: usize) -> (Vec<Placed>, usize) {
        let mut reach = 0_usize;
        let placed = self
            .nodes
            .iter()
            .map(|node| {
                let mut offsets = [[0_u32; 4]; 3];
                let mut weights = [0.0_f32; 3];
                let mut count = 0_u8;
                if let Some(f) = self.features.get(node.feature) {
                    count = f.count as u8;
                    for ((slot, weight), rect) in offsets
                        .iter_mut()
                        .zip(weights.iter_mut())
                        .zip(f.rects.iter())
                        .take(f.count)
                    {
                        let x = rect.x as usize;
                        let y = rect.y as usize;
                        let w = rect.w as usize;
                        let h = rect.h as usize;
                        let corners = [
                            y * stride + x,
                            y * stride + x + w,
                            (y + h) * stride + x,
                            (y + h) * stride + x + w,
                        ];
                        reach = reach.max(corners[3]);
                        *slot = corners.map(|c| c as u32);
                        *weight = rect.weight;
                    }
                }
                Placed {
                    offsets,
                    weights,
                    count,
                    threshold: node.threshold,
                    left: node.left,
                    right: node.right,
                }
            })
            .collect();
        (placed, reach)
    }

    /// One window: `1` for a pass, `-stage` for a rejection at that stage (so `0` is a
    /// rejection at the first), `-1` for a window too flat to normalise.
    fn run_at(
        &self,
        integral: &Integral,
        placed: &[Placed],
        reach: usize,
        x: usize,
        y: usize,
    ) -> i32 {
        let stride = integral.stride;
        // OpenCV's normalisation rectangle: the window less a one-pixel border.
        let nw = self.width.saturating_sub(2) as usize;
        let nh = self.height.saturating_sub(2) as usize;
        let area = (nw * nh) as f64;
        let valsum = Integral::rect(&integral.sum, stride, x + 1, y + 1, nw, nh) as f64;
        let valsq = Integral::rect(&integral.sq, stride, x + 1, y + 1, nw, nh) as f64;
        let nf = area * valsq - valsum * valsum;
        if nf <= 0.0 {
            return -1;
        }
        let norm = (1.0 / nf.sqrt()) as f32;
        if f64::from(norm) * area >= 0.1 {
            return -1;
        }
        let origin = y * stride + x;
        // The whole window's corners, borrowed once. Every offset in `placed` is at most
        // `reach`, so a window that fits is a window every node can read.
        let Some(window) = integral.sum.get(origin..=origin + reach) else {
            return -1;
        };

        for (index, stage) in self.stages.iter().enumerate() {
            let mut total = 0.0_f32;
            for tree in self
                .trees
                .get(stage.first_tree..stage.first_tree + stage.tree_count)
                .unwrap_or_default()
            {
                let mut idx: i32 = 0;
                loop {
                    let Some(node) = placed.get(tree.first_node + idx as usize) else {
                        return -(index as i32);
                    };
                    let value = evaluate(window, node);
                    idx = if value * norm < node.threshold {
                        node.left
                    } else {
                        node.right
                    };
                    if idx <= 0 || idx as usize >= tree.node_count {
                        break;
                    }
                }
                let leaf = if idx <= 0 { (-idx) as usize } else { 0 };
                total += self
                    .leaves
                    .get(tree.first_leaf + leaf)
                    .copied()
                    .unwrap_or(0.0);
            }
            if total < stage.threshold {
                return -(index as i32);
            }
        }
        1
    }
}

/// The weighted rectangle sum of one placed node over one window.
#[inline]
fn evaluate(window: &[i32], node: &Placed) -> f32 {
    let at = |i: u32| -> i32 { window.get(i as usize).copied().unwrap_or(0) };
    let mut value = 0.0_f32;
    for (o, w) in node
        .offsets
        .iter()
        .zip(node.weights.iter())
        .take(usize::from(node.count))
    {
        let rect = at(o[3]) - at(o[1]) - at(o[2]) + at(o[0]);
        value += w * rect as f32;
    }
    value
}

fn parse_tree(line: &str) -> Result<(Vec<Node>, Vec<f32>), CascadeError> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.first() != Some(&"t") {
        return Err(CascadeError(format!("expected a tree, found `{line}`")));
    }
    let count = number::<usize>(parts.get(1))?;
    let mut nodes = Vec::with_capacity(count);
    for n in 0..count {
        let at = 2 + n * 4;
        nodes.push(Node {
            left: number(parts.get(at))?,
            right: number(parts.get(at + 1))?,
            feature: number(parts.get(at + 2))?,
            threshold: number(parts.get(at + 3))?,
        });
    }
    let mut leaves = Vec::with_capacity(count + 1);
    for l in 0..=count {
        leaves.push(number(parts.get(2 + count * 4 + l))?);
    }
    if parts.len() != 2 + count * 4 + count + 1 {
        return Err(CascadeError(format!("tree has trailing values: `{line}`")));
    }
    if nodes.iter().any(|n| {
        n.left >= count as i32
            || n.right >= count as i32
            || n.left < -(count as i32)
            || n.right < -(count as i32)
    }) {
        return Err(CascadeError(format!(
            "a child index is out of range: `{line}`"
        )));
    }
    Ok((nodes, leaves))
}

fn number<T: std::str::FromStr>(word: Option<&&str>) -> Result<T, CascadeError> {
    let word = word.ok_or_else(|| CascadeError("a line is too short".to_string()))?;
    word.parse::<T>()
        .map_err(|_| CascadeError(format!("`{word}` is not a number")))
}

/// OpenCV's `groupRectangles`: partition by similarity, average each class, keep classes with
/// more than `threshold` members, and drop a small class lying inside a larger confident one.
#[must_use]
pub fn group_rectangles(rects: &[[i32; 4]], threshold: u32, eps: f64) -> Vec<Detection> {
    if rects.is_empty() {
        return Vec::new();
    }
    let n = rects.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for i in 0..n {
        for j in (i + 1)..n {
            let (Some(a), Some(b)) = (rects.get(i), rects.get(j)) else {
                continue;
            };
            if similar(a, b, eps) {
                let ra = find(&mut parent, i);
                let rb = find(&mut parent, j);
                if ra != rb {
                    let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
                    if let Some(slot) = parent.get_mut(hi) {
                        *slot = lo;
                    }
                }
            }
        }
    }
    // Classes in order of their first member, which is OpenCV's label order.
    let mut class_of_root: Vec<Option<usize>> = vec![None; n];
    let mut sums: Vec<[i64; 4]> = Vec::new();
    let mut counts: Vec<u32> = Vec::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        let class = if let Some(Some(c)) = class_of_root.get(root) {
            *c
        } else {
            sums.push([0; 4]);
            counts.push(0);
            let c = sums.len() - 1;
            if let Some(slot) = class_of_root.get_mut(root) {
                *slot = Some(c);
            }
            c
        };
        if let (Some(sum), Some(count), Some(r)) =
            (sums.get_mut(class), counts.get_mut(class), rects.get(i))
        {
            for (s, v) in sum.iter_mut().zip(r.iter()) {
                *s += i64::from(*v);
            }
            *count += 1;
        }
    }
    let averaged: Vec<[i32; 4]> = sums
        .iter()
        .zip(counts.iter())
        .map(|(s, c)| {
            let k = 1.0 / f64::from((*c).max(1));
            [
                (s[0] as f64 * k).round() as i32,
                (s[1] as f64 * k).round() as i32,
                (s[2] as f64 * k).round() as i32,
                (s[3] as f64 * k).round() as i32,
            ]
        })
        .collect();

    let mut out = Vec::new();
    for (i, r1) in averaged.iter().enumerate() {
        let n1 = counts.get(i).copied().unwrap_or(0);
        if n1 <= threshold {
            continue;
        }
        let mut inside = false;
        for (j, r2) in averaged.iter().enumerate() {
            let n2 = counts.get(j).copied().unwrap_or(0);
            if j == i || n2 <= threshold {
                continue;
            }
            let dx = (f64::from(r2[2]) * eps) as i32;
            let dy = (f64::from(r2[3]) * eps) as i32;
            if r1[0] >= r2[0] - dx
                && r1[1] >= r2[1] - dy
                && r1[0] + r1[2] <= r2[0] + r2[2] + dx
                && r1[1] + r1[3] <= r2[1] + r2[3] + dy
                && (n2 > n1.max(3) || n1 < 3)
            {
                inside = true;
                break;
            }
        }
        if !inside {
            out.push(Detection {
                x: r1[0] as f32,
                y: r1[1] as f32,
                w: r1[2] as f32,
                h: r1[3] as f32,
                neighbours: n1,
            });
        }
    }
    out
}

/// The root of a union-find set, compressing the path on the way.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent.get(i).copied().unwrap_or(i) != i {
        let up = parent.get(i).copied().unwrap_or(i);
        let grand = parent.get(up).copied().unwrap_or(up);
        if let Some(slot) = parent.get_mut(i) {
            *slot = grand;
        }
        i = grand;
    }
    i
}

fn similar(a: &[i32; 4], b: &[i32; 4], eps: f64) -> bool {
    let delta = eps * f64::from(a[2].min(b[2]) + a[3].min(b[3])) * 0.5;
    f64::from((a[0] - b[0]).abs()) <= delta
        && f64::from((a[1] - b[1]).abs()) <= delta
        && f64::from((a[0] + a[2] - b[0] - b[2]).abs()) <= delta
        && f64::from((a[1] + a[3] - b[1] - b[3]).abs()) <= delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_shipped_cascades_parse() {
        let frontal = Cascade::frontal().expect("frontal cascade parses");
        assert_eq!(frontal.window(), (20, 20));
        assert_eq!(frontal.stage_count(), 20);
        let profile = Cascade::profile().expect("profile cascade parses");
        assert_eq!(profile.stage_count(), 26);
        let eye = Cascade::eye().expect("eye cascade parses");
        assert_eq!(eye.stage_count(), 24);
    }

    #[test]
    fn a_flat_image_has_no_faces() {
        let image = GreyImage::new(96, 96, vec![128; 96 * 96]).unwrap();
        let found = Cascade::frontal()
            .unwrap()
            .detect(&image, ScanParams::default());
        assert!(found.is_empty());
    }

    #[test]
    fn noise_has_no_faces() {
        let mut state = 0x1234_5678_u32;
        let pixels: Vec<u8> = (0..120 * 120)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state % 256) as u8
            })
            .collect();
        let image = GreyImage::new(120, 120, pixels).unwrap();
        let found = Cascade::frontal()
            .unwrap()
            .detect(&image, ScanParams::default());
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn grouping_merges_neighbours_and_drops_lonely_windows() {
        let mut rects = Vec::new();
        for d in 0..5 {
            rects.push([100 + d, 100 - d, 40, 40]);
        }
        rects.push([10, 10, 40, 40]);
        let grouped = group_rectangles(&rects, 3, 0.2);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].neighbours, 5);
        assert!((grouped[0].x - 102.0).abs() < 1.0);
    }

    #[test]
    fn a_bad_file_is_an_error_not_a_panic() {
        assert!(Cascade::parse("").is_err());
        assert!(Cascade::parse("cascade 20 20 1 0\ns 0.5 1\nt 1 0 -1 3 0.1 0.2 0.3").is_err());
        assert!(Cascade::parse("cascade x 20 1 0").is_err());
    }

    #[test]
    fn the_mirror_of_a_mirror_is_the_original() {
        let image = GreyImage::new(3, 2, vec![1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(image.mirrored().pixels, vec![3, 2, 1, 6, 5, 4]);
        assert_eq!(image.mirrored().mirrored(), image);
        assert_eq!(image.crop(1, 0, 2, 2).pixels, vec![2, 3, 5, 6]);
    }
}
