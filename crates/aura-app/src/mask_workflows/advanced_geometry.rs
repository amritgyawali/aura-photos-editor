//! Coordinate-defined shapes and placements on the bounded mask grid.
use super::{advanced::Options, authoring::Transform, check, invalid, plane::Plane, Action};
use aura_core::{progress::CancelToken, AuraResult};

impl Plane {
    // Validated grid dimensions are <=512; normalized finite coordinates are clamped before casts.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::indexing_slicing
    )]
    pub(super) fn geometry(
        &mut self,
        action: Action,
        options: &Options,
        other: Option<&Self>,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        match action {
            Action::AlignCentroid => {
                let source = self
                    .centroid()
                    .ok_or_else(|| invalid("The source selection is empty."))?;
                let target = other
                    .and_then(Self::centroid)
                    .ok_or_else(|| invalid("The destination selection is empty."))?;
                return self.transform(
                    Transform {
                        offset: [target[0] - source[0], target[1] - source[1]],
                        scale: [1.0, 1.0],
                        angle: 0.0,
                    },
                    cancel,
                );
            }
            Action::FitBounds => return self.fit_bounds(options, cancel),
            Action::PerspectiveSelection => return self.perspective(&options.vertices, cancel),
            Action::PolygonMarquee if options.vertices.len() < 3 => {
                return Err(invalid("A polygon needs 3-128 vertices."))
            }
            _ => {}
        }
        let radius = ((options.bounds[2] - options.bounds[0])
            .min(options.bounds[3] - options.bounds[1])
            * options.corner)
            .min(
                (options.bounds[2] - options.bounds[0]).min(options.bounds[3] - options.bounds[1])
                    * 0.5,
            );
        let centre = [
            (options.bounds[0] + options.bounds[2]) * 0.5,
            (options.bounds[1] + options.bounds[3]) * 0.5,
        ];
        let radii = [
            (options.bounds[2] - options.bounds[0]) * 0.5,
            (options.bounds[3] - options.bounds[1]) * 0.5,
        ];
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                // Four subcell samples antialias newly authored hard boundaries.
                let mut covered = 0_u16;
                for dy in [0.25, 0.75] {
                    for dx in [0.25, 0.75] {
                        let position = [
                            (x as f32 + dx) / self.width as f32,
                            (y as f32 + dy) / self.height as f32,
                        ];
                        let inside = match action {
                            Action::PolygonMarquee => polygon_contains(&options.vertices, position),
                            Action::RoundedMarquee => {
                                let q = [
                                    position[0].clamp(
                                        options.bounds[0] + radius,
                                        options.bounds[2] - radius,
                                    ),
                                    position[1].clamp(
                                        options.bounds[1] + radius,
                                        options.bounds[3] - radius,
                                    ),
                                ];
                                position[0] >= options.bounds[0]
                                    && position[0] <= options.bounds[2]
                                    && position[1] >= options.bounds[1]
                                    && position[1] <= options.bounds[3]
                                    && (position[0] - q[0]).hypot(position[1] - q[1])
                                        <= radius + f32::EPSILON
                            }
                            Action::SectorMarquee => {
                                let dx = (position[0] - centre[0]) / radii[0];
                                let dy = (position[1] - centre[1]) / radii[1];
                                let angle =
                                    (dy.atan2(dx).to_degrees() - options.angle).rem_euclid(360.0);
                                dx * dx + dy * dy <= 1.0 && angle <= options.sweep
                            }
                            _ => return Err(invalid("Unknown shape operation.")),
                        };
                        if inside {
                            covered += 1;
                        }
                    }
                }
                self.alpha[y * self.width + x] = ((covered * 255 + 2) / 4) as u8;
            }
        }
        Ok(())
    }
    // Source bounds and target coordinates are validated on the <=512-cell selection grid.
    #[allow(clippy::indexing_slicing, clippy::cast_precision_loss)]
    fn fit_bounds(&mut self, options: &Options, cancel: &CancelToken) -> AuraResult<()> {
        let bounds = self
            .selected_bounds()
            .ok_or_else(|| invalid("The source selection is empty."))?;
        let mut output = vec![0; self.alpha.len()];
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                let position = [
                    (x as f32 + 0.5) / self.width as f32,
                    (y as f32 + 0.5) / self.height as f32,
                ];
                if position[0] >= options.bounds[0]
                    && position[0] < options.bounds[2]
                    && position[1] >= options.bounds[1]
                    && position[1] < options.bounds[3]
                {
                    let u = bounds[0]
                        + (position[0] - options.bounds[0])
                            / (options.bounds[2] - options.bounds[0])
                            * (bounds[2] - bounds[0]);
                    let v = bounds[1]
                        + (position[1] - options.bounds[1])
                            / (options.bounds[3] - options.bounds[1])
                            * (bounds[3] - bounds[1]);
                    output[y * self.width + x] = self.sample(u, v);
                }
            }
        }
        self.alpha = output;

        Ok(())
    }
    #[allow(clippy::cast_precision_loss)] // Bounded grid indices have exact f32 integer representation.
    pub(super) fn centroid(&self) -> Option<[f32; 2]> {
        let (mut sum, mut x, mut y) = (0.0_f32, 0.0_f32, 0.0_f32);
        for (index, a) in self.alpha.iter().enumerate() {
            let a = f32::from(*a);
            sum += a;
            x += a * (index % self.width) as f32;
            y += a * (index / self.width) as f32;
        }
        (sum > 0.0).then(|| {
            [
                (x / sum + 0.5) / self.width as f32,
                (y / sum + 0.5) / self.height as f32,
            ]
        })
    }
    #[allow(clippy::cast_precision_loss)] // Bounds derive solely from the <=512 cell selection grid.
    pub(super) fn selected_bounds(&self) -> Option<[f32; 4]> {
        let (mut left, mut top, mut right, mut bottom) = (self.width, self.height, 0, 0);
        for (i, a) in self.alpha.iter().enumerate().filter(|(_, a)| **a >= 128) {
            let _ = a;
            let (x, y) = (i % self.width, i / self.width);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
        (right > left && bottom > top).then(|| {
            [
                left as f32 / self.width as f32,
                top as f32 / self.height as f32,
                right as f32 / self.width as f32,
                bottom as f32 / self.height as f32,
            ]
        })
    }
    // Finite normalized input is checked before converting to bounded cell indices.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    pub(super) fn sample(&self, u: f32, v: f32) -> u8 {
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return 0;
        }
        let x = (u * self.width as f32) as usize;
        let y = (v * self.height as f32) as usize;
        self.alpha.get(y * self.width + x).copied().unwrap_or(0)
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::indexing_slicing
    )] // Validated grid and four point homography dimensions.
    fn perspective(&mut self, vertices: &[[f32; 2]], cancel: &CancelToken) -> AuraResult<()> {
        if vertices.len() != 4 {
            return Err(invalid(
                "Perspective placement needs exactly four clockwise corners.",
            ));
        }
        for i in 0..4 {
            let a = vertices[i];
            let bounds = vertices[(i + 1) % 4];
            let c = vertices[(i + 2) % 4];
            if (bounds[0] - a[0]) * (c[1] - bounds[1]) - (bounds[1] - a[1]) * (c[0] - bounds[0])
                <= 0.0001
            {
                return Err(invalid(
                    "Perspective corners must form a convex clockwise quadrilateral.",
                ));
            }
        }
        let unit = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let mut equations = [[0.0_f64; 9]; 8];
        for i in 0..4 {
            let (x, y) = (f64::from(vertices[i][0]), f64::from(vertices[i][1]));
            let (u, v) = (unit[i][0], unit[i][1]);
            equations[2 * i] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u];
            equations[2 * i + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v];
        }
        for column in 0..8 {
            let pivot = (column..8)
                .max_by(|a, bounds| {
                    equations[*a][column]
                        .abs()
                        .total_cmp(&equations[*bounds][column].abs())
                })
                .ok_or_else(|| invalid("Invalid perspective points."))?;
            if equations[pivot][column].abs() < 1e-10 {
                return Err(invalid("Degenerate perspective placement."));
            }
            equations.swap(column, pivot);
            let divisor = equations[column][column];
            for entry in &mut equations[column][column..9] {
                *entry /= divisor;
            }
            for row in 0..8 {
                if row == column {
                    continue;
                }
                let factor = equations[row][column];
                let pivot_row = equations[column];
                for (entry, pivot_value) in equations[row].iter_mut().zip(pivot_row).skip(column) {
                    *entry -= factor * pivot_value;
                }
            }
        }
        let h = equations.map(|row| row[8]);
        let mut output = vec![0; self.alpha.len()];
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                let (px, py) = (
                    (x as f64 + 0.5) / self.width as f64,
                    (y as f64 + 0.5) / self.height as f64,
                );
                let denominator = h[6] * px + h[7] * py + 1.0;
                if denominator.abs() > 1e-10 {
                    output[y * self.width + x] = self.sample(
                        ((h[0] * px + h[1] * py + h[2]) / denominator) as f32,
                        ((h[3] * px + h[4] * py + h[5]) / denominator) as f32,
                    );
                }
            }
        }
        self.alpha = output;
        Ok(())
    }
}
#[allow(clippy::indexing_slicing)] // Caller validates a bounded polygon of at least three points.
fn polygon_contains(vertices: &[[f32; 2]], position: [f32; 2]) -> bool {
    let mut inside = false;
    for i in 0..vertices.len() {
        let a = vertices[i];
        let bounds = vertices[(i + 1) % vertices.len()];
        if (a[1] > position[1]) != (bounds[1] > position[1])
            && position[0] < (bounds[0] - a[0]) * (position[1] - a[1]) / (bounds[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
