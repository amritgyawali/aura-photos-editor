//! Connected topology, distance fields, thinning and automatic coverage thresholding.
use super::{advanced::Options, check, invalid, plane::Plane, Action};
use aura_core::{progress::CancelToken, AuraResult};
use std::collections::VecDeque;
impl Plane {
    pub(super) fn isolate(&self, group: &[usize]) -> Self {
        let mut alpha = vec![0; self.alpha.len()];
        for index in group {
            if let (Some(destination), Some(source)) =
                (alpha.get_mut(*index), self.alpha.get(*index))
            {
                *destination = *source;
            }
        }
        Self {
            width: self.width,
            height: self.height,
            alpha,
        }
    }
    pub(super) fn region_groups(&self, cancel: &CancelToken) -> AuraResult<Vec<Vec<usize>>> {
        let mut visited = vec![false; self.alpha.len()];
        let mut groups = Vec::new();
        for (index, a) in self.alpha.iter().enumerate() {
            if index % self.width == 0 {
                check(cancel)?;
            }
            if *a >= 128 && !visited.get(index).copied().unwrap_or(true) {
                groups.push(self.group(index, &mut visited, cancel)?);
            }
        }
        Ok(groups)
    }
    pub(super) fn neighbors(&self, index: usize) -> impl Iterator<Item = usize> {
        let (x, y) = (index % self.width, index / self.width);
        [
            x.checked_sub(1).map(|v| y * self.width + v),
            (x + 1 < self.width).then_some(index + 1),
            y.checked_sub(1).map(|v| v * self.width + x),
            (y + 1 < self.height).then_some(index + self.width),
        ]
        .into_iter()
        .flatten()
    }
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )] // Checked <=512 grid; quantized alpha is clamped before casting.
    pub(super) fn topology(
        &mut self,
        action: Action,
        o: &Options,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        match action {
            Action::NegativeSpace => self.negative_space(cancel)?,

            Action::InteriorFalloff => self.interior_falloff(o, cancel)?,

            Action::AutoThreshold => self.auto_threshold(),

            Action::VoronoiPartition => {
                if o.vertices.len() < 2 || o.seed_index >= o.vertices.len() {
                    return Err(invalid(
                        "Partition needs at least two seeds and an existing seed index.",
                    ));
                }
                for y in 0..self.height {
                    check(cancel)?;
                    for x in 0..self.width {
                        let p = [
                            (x as f32 + 0.5) / self.width as f32,
                            (y as f32 + 0.5) / self.height as f32,
                        ];
                        let winner = o
                            .vertices
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                let d =
                                    |s: &[f32; 2]| (s[0] - p[0]).powi(2) + (s[1] - p[1]).powi(2);
                                d(a).total_cmp(&d(b))
                            })
                            .map(|(i, _)| i);
                        if winner != Some(o.seed_index) {
                            self.alpha[y * self.width + x] = 0;
                        }
                    }
                }
            }
            Action::FilterRegions => {
                let mut retained = vec![0; self.alpha.len()];
                for group in self.region_groups(cancel)? {
                    let mut perimeter = 0_u32;
                    for i in &group {
                        let count = self.neighbors(*i).filter(|n| self.alpha[*n] >= 128).count();
                        perimeter += 4 - count as u32;
                    }
                    let compactness = 4.0 * std::f32::consts::PI * group.len() as f32
                        / (perimeter.max(1) as f32).powi(2);
                    if group.len() >= o.min_area
                        && group.len() <= o.max_area
                        && compactness >= o.compactness
                    {
                        for i in group {
                            retained[i] = self.alpha[i];
                        }
                    }
                }
                self.alpha = retained;
            }
            Action::Skeleton => return self.thin(cancel),
            _ => return Err(invalid("Unknown topology operation.")),
        }
        check(cancel)
    }
    #[allow(clippy::indexing_slicing)] // The padded frame is bounded by 514 cells per edge.
    fn thin(&mut self, cancel: &CancelToken) -> AuraResult<()> {
        check(cancel)?;
        // A zero border lets the same thinning rule process selections that touch the
        // photo edge. Keeping the original edge fixed leaves whole blocks unthinned.
        let mut padded = Self {
            width: self.width + 2,
            height: self.height + 2,
            alpha: vec![0; (self.width + 2) * (self.height + 2)],
        };
        for y in 0..self.height {
            let start = (y + 1) * padded.width + 1;
            padded.alpha[start..start + self.width]
                .copy_from_slice(&self.alpha[y * self.width..(y + 1) * self.width]);
        }
        padded.thin_interior(cancel)?;
        for y in 0..self.height {
            let start = (y + 1) * padded.width + 1;
            self.alpha[y * self.width..(y + 1) * self.width]
                .copy_from_slice(&padded.alpha[start..start + self.width]);
        }
        check(cancel)
    }

    #[allow(clippy::indexing_slicing)] // Every neighbor is derived from an interior bounded-grid cell.
    fn thin_interior(&mut self, cancel: &CancelToken) -> AuraResult<()> {
        for a in &mut self.alpha {
            *a = if *a >= 128 { 255 } else { 0 };
        }
        if self.width < 3 || self.height < 3 {
            return Ok(());
        }
        for _ in 0..self.width.max(self.height) {
            let mut changed = false;
            for pass in [false, true] {
                let mut remove = Vec::new();
                for y in 1..self.height - 1 {
                    check(cancel)?;
                    for x in 1..self.width - 1 {
                        let i = y * self.width + x;
                        if self.alpha[i] == 0 {
                            continue;
                        }
                        let n = [
                            i - self.width,
                            i - self.width + 1,
                            i + 1,
                            i + self.width + 1,
                            i + self.width,
                            i + self.width - 1,
                            i - 1,
                            i - self.width - 1,
                        ]
                        .map(|n| u8::from(self.alpha[n] > 0));
                        let count = n.iter().sum::<u8>();
                        let transitions =
                            (0..8).filter(|j| n[*j] == 0 && n[(*j + 1) % 8] > 0).count();
                        let removable = if pass {
                            n[0] * n[2] * n[6] == 0 && n[0] * n[4] * n[6] == 0
                        } else {
                            n[0] * n[2] * n[4] == 0 && n[2] * n[4] * n[6] == 0
                        };
                        if (2..=6).contains(&count) && transitions == 1 && removable {
                            remove.push(i);
                        }
                    }
                }
                for i in remove {
                    self.alpha[i] = 0;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(())
    }

    // Coordinates and indices are confined to the validated <=512-cell grid.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn negative_space(&mut self, cancel: &CancelToken) -> AuraResult<()> {
        let mut reachable = vec![false; self.alpha.len()];
        let mut queue = VecDeque::new();
        for (i, a) in self.alpha.iter().enumerate() {
            let (x, y) = (i % self.width, i / self.width);
            if *a < 128 && (x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height) {
                reachable[i] = true;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            if i % 1024 == 0 {
                check(cancel)?;
            }
            for n in self.neighbors(i) {
                if !reachable[n] && self.alpha[n] < 128 {
                    reachable[n] = true;
                    queue.push_back(n);
                }
            }
        }
        for (i, a) in self.alpha.iter_mut().enumerate() {
            *a = if !reachable[i] && *a < 128 {
                255 - *a
            } else {
                0
            };
        }

        Ok(())
    }

    // Coordinates and indices are confined to the validated <=512-cell grid.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn interior_falloff(&mut self, o: &Options, cancel: &CancelToken) -> AuraResult<()> {
        let mut distance = vec![u32::MAX; self.alpha.len()];
        let mut queue = VecDeque::new();
        for (i, a) in self.alpha.iter().enumerate() {
            let (x, y) = (i % self.width, i / self.width);
            if *a < 128 {
                distance[i] = 0;
                queue.push_back(i);
            } else if x == 0 || y == 0 || x + 1 == self.width || y + 1 == self.height {
                distance[i] = 1;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            if i % 1024 == 0 {
                check(cancel)?;
            }
            for n in self.neighbors(i) {
                if distance[n] > distance[i] + 1 {
                    distance[n] = distance[i] + 1;
                    queue.push_back(n);
                }
            }
        }
        for (i, a) in self.alpha.iter_mut().enumerate() {
            *a = ((u32::from(*a) * distance[i].min(o.radius) + o.radius / 2) / o.radius) as u8;
        }

        Ok(())
    }

    // Coordinates and indices are confined to the validated <=512-cell grid.
    #[allow(
        clippy::indexing_slicing,
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn auto_threshold(&mut self) {
        let mut hist = [0_u32; 256];
        for a in &self.alpha {
            hist[usize::from(*a)] += 1;
        }
        let total = self.alpha.len() as f64;
        let sum = self.alpha.iter().map(|a| f64::from(*a)).sum::<f64>();
        let (mut weight, mut partial, mut best, mut threshold) = (0.0, 0.0, -1.0, 127_u8);
        for (i, count) in hist.iter().enumerate() {
            weight += f64::from(*count);
            if weight == 0.0 {
                continue;
            }
            let remaining = total - weight;
            if remaining == 0.0 {
                break;
            }
            partial += i as f64 * f64::from(*count);
            let delta = partial / weight - (sum - partial) / remaining;
            let between = weight * remaining * delta * delta;
            if between > best {
                best = between;
                threshold = i as u8;
            }
        }
        for a in &mut self.alpha {
            *a = if *a > threshold { 255 } else { 0 };
        }
    }
}
