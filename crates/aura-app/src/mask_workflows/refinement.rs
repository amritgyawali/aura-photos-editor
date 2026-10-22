//! Independent, bounded-grid selection placement and topology tools.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
use super::{authoring::Transform, check, invalid, plane::Plane};
use aura_core::{progress::CancelToken, AuraResult};
use std::collections::VecDeque;

impl Plane {
    pub(super) fn transform(
        &mut self,
        transform: Transform,
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        if transform
            .offset
            .iter()
            .any(|v| !v.is_finite() || !(-1.0..=1.0).contains(v))
            || transform
                .scale
                .iter()
                .any(|v| !v.is_finite() || !(0.1..=4.0).contains(v))
            || !transform.angle.is_finite()
            || !(-360.0..=360.0).contains(&transform.angle)
        {
            return Err(invalid("Selection offsets must be within one frame, scale 0.1–4, and angle within 360 degrees."));
        }
        let (sin, cos) = transform.angle.to_radians().sin_cos();
        let mut output = vec![0; self.alpha.len()];
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                let dx = (x as f32 + 0.5) / self.width as f32 - 0.5 - transform.offset[0];
                let dy = (y as f32 + 0.5) / self.height as f32 - 0.5 - transform.offset[1];
                let u = (cos * dx + sin * dy) / transform.scale[0] + 0.5;
                let v = (-sin * dx + cos * dy) / transform.scale[1] + 0.5;
                if (0.0..1.0).contains(&u) && (0.0..1.0).contains(&v) {
                    let sx = u * self.width as f32 - 0.5;
                    let sy = v * self.height as f32 - 0.5;
                    let mut sum = 0.0;
                    for (iy, wy) in [
                        (sy.floor(), 1.0 - sy.fract().rem_euclid(1.0)),
                        (sy.floor() + 1.0, sy.fract().rem_euclid(1.0)),
                    ] {
                        for (ix, wx) in [
                            (sx.floor(), 1.0 - sx.fract().rem_euclid(1.0)),
                            (sx.floor() + 1.0, sx.fract().rem_euclid(1.0)),
                        ] {
                            let ix = ix.clamp(0.0, (self.width - 1) as f32) as usize;
                            let iy = iy.clamp(0.0, (self.height - 1) as f32) as usize;
                            sum += f32::from(self.alpha[iy * self.width + ix]) * wx * wy;
                        }
                    }
                    output[y * self.width + x] = sum.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        self.alpha = output;
        Ok(())
    }
    pub(super) fn median(&mut self, radius: usize, cancel: &CancelToken) -> AuraResult<()> {
        if radius == 0 {
            return Ok(());
        }
        let mut output = self.alpha.clone();
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                let mut neighbors = Vec::with_capacity((radius * 2 + 1).pow(2));
                for sy in y.saturating_sub(radius)..(y + radius + 1).min(self.height) {
                    for sx in x.saturating_sub(radius)..(x + radius + 1).min(self.width) {
                        neighbors.push(self.alpha[sy * self.width + sx]);
                    }
                }
                let middle = neighbors.len() / 2;
                neighbors.select_nth_unstable(middle);
                output[y * self.width + x] = neighbors[middle];
            }
        }
        self.alpha = output;
        Ok(())
    }
    pub(super) fn group(
        &self,
        start: usize,
        visited: &mut [bool],
        cancel: &CancelToken,
    ) -> AuraResult<Vec<usize>> {
        let mut queue = VecDeque::from([start]);
        let mut group = Vec::new();
        visited[start] = true;
        while let Some(index) = queue.pop_front() {
            if group.len() % 1024 == 0 {
                check(cancel)?;
            }
            group.push(index);
            let (x, y) = (index % self.width, index / self.width);
            for neighbor in [
                x.checked_sub(1).map(|n| y * self.width + n),
                (x + 1 < self.width).then_some(index + 1),
                y.checked_sub(1).map(|n| n * self.width + x),
                (y + 1 < self.height).then_some(index + self.width),
            ]
            .into_iter()
            .flatten()
            {
                if !visited[neighbor] && self.alpha[neighbor] >= 128 {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        Ok(group)
    }
    pub(super) fn keep_connected(
        &mut self,
        point: [f32; 2],
        cancel: &CancelToken,
    ) -> AuraResult<()> {
        if point
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err(invalid("Seed coordinates must be within the photo."));
        }
        let x = ((point[0] * self.width as f32) as usize).min(self.width - 1);
        let y = ((point[1] * self.height as f32) as usize).min(self.height - 1);
        if self.alpha[y * self.width + x] < 128 {
            return Err(invalid(
                "The seed point is outside the selection at 50% coverage.",
            ));
        }
        let group = self.group(
            y * self.width + x,
            &mut vec![false; self.alpha.len()],
            cancel,
        )?;
        self.retain_group(&group);
        Ok(())
    }
    pub(super) fn keep_largest(&mut self, cancel: &CancelToken) -> AuraResult<()> {
        let mut visited = vec![false; self.alpha.len()];
        let mut largest = Vec::new();
        for index in 0..self.alpha.len() {
            if index % self.width == 0 {
                check(cancel)?;
            }
            if !visited[index] && self.alpha[index] >= 128 {
                let group = self.group(index, &mut visited, cancel)?;
                if group.len() > largest.len() {
                    largest = group;
                }
            }
        }
        self.retain_group(&largest);
        Ok(())
    }
    fn retain_group(&mut self, group: &[usize]) {
        let mut retained = vec![0; self.alpha.len()];
        for index in group {
            retained[*index] = self.alpha[*index];
        }
        self.alpha = retained;
    }
    pub(super) fn convex_hull(&mut self, cancel: &CancelToken) -> AuraResult<()> {
        let mut points: Vec<(i64, i64)> = self
            .alpha
            .iter()
            .enumerate()
            .filter(|(_, a)| **a >= 128)
            .map(|(i, _)| {
                (
                    i64::from((i % self.width) as u32),
                    i64::from((i / self.width) as u32),
                )
            })
            .collect();
        points.sort_unstable();
        if points.len() < 3 {
            return Ok(());
        }
        let cross = |a: (i64, i64), b: (i64, i64), c: (i64, i64)| {
            (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
        };
        let chain = |points: Vec<(i64, i64)>| {
            let mut out = Vec::new();
            for p in points {
                while out.len() >= 2 && cross(out[out.len() - 2], out[out.len() - 1], p) <= 0 {
                    out.pop();
                }
                out.push(p);
            }
            out.pop();
            out
        };
        let mut hull = chain(points.clone());
        hull.extend(chain(points.into_iter().rev().collect()));
        if hull.len() < 3 {
            return Ok(());
        }
        for y in 0..self.height {
            check(cancel)?;
            for x in 0..self.width {
                let p = (i64::from(x as u32), i64::from(y as u32));
                let inside = hull
                    .iter()
                    .enumerate()
                    .all(|(i, a)| cross(*a, hull[(i + 1) % hull.len()], p) >= 0);
                self.alpha[y * self.width + x] = if inside { 255 } else { 0 };
            }
        }
        Ok(())
    }
}
