//! EXIF orientation, applied once and never again.
//!
//! A camera held vertically records horizontal pixels plus a tag saying "turn
//! this". Every consumer that forgets the tag shows a sideways bride. Rather
//! than passing orientation down the whole pipeline and hoping, AURA rotates the
//! pixels once, at the top of tier 1 and tier 2, and everything downstream works
//! in display orientation with `orientation = 1`.
//!
//! The transform is exact: pixels are moved, never resampled, so rotating a
//! preview costs a memcpy per row and loses nothing.

/// The eight EXIF orientation values, named by what they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    /// 1 - already upright.
    Normal,
    /// 2 - mirrored left to right.
    FlipHorizontal,
    /// 3 - rotated 180 degrees.
    Rotate180,
    /// 4 - mirrored top to bottom.
    FlipVertical,
    /// 5 - transposed across the main diagonal.
    Transpose,
    /// 6 - rotated 90 degrees clockwise.
    Rotate90,
    /// 7 - transposed across the anti-diagonal.
    Transverse,
    /// 8 - rotated 270 degrees clockwise.
    Rotate270,
}

impl Orientation {
    /// Interpret an EXIF value. Anything outside 1 to 8 is treated as upright,
    /// because guessing at a corrupt tag turns one bad file into a rotated one.
    #[must_use]
    pub const fn from_exif(value: u16) -> Self {
        match value {
            2 => Self::FlipHorizontal,
            3 => Self::Rotate180,
            4 => Self::FlipVertical,
            5 => Self::Transpose,
            6 => Self::Rotate90,
            7 => Self::Transverse,
            8 => Self::Rotate270,
            _ => Self::Normal,
        }
    }

    /// The EXIF number this orientation corresponds to.
    #[must_use]
    pub const fn to_exif(self) -> u16 {
        match self {
            Self::Normal => 1,
            Self::FlipHorizontal => 2,
            Self::Rotate180 => 3,
            Self::FlipVertical => 4,
            Self::Transpose => 5,
            Self::Rotate90 => 6,
            Self::Transverse => 7,
            Self::Rotate270 => 8,
        }
    }

    /// True when applying this orientation exchanges width and height.
    #[must_use]
    pub const fn swaps_axes(self) -> bool {
        matches!(
            self,
            Self::Transpose | Self::Rotate90 | Self::Transverse | Self::Rotate270
        )
    }

    /// True when there is nothing to do.
    #[must_use]
    pub const fn is_identity(self) -> bool {
        matches!(self, Self::Normal)
    }

    /// Where the source pixel for destination `(x, y)` lives.
    const fn source_of(self, x: u32, y: u32, width: u32, height: u32) -> (u32, u32) {
        // `width` and `height` are the *source* dimensions.
        match self {
            Self::Normal => (x, y),
            Self::FlipHorizontal => (width - 1 - x, y),
            Self::Rotate180 => (width - 1 - x, height - 1 - y),
            Self::FlipVertical => (x, height - 1 - y),
            Self::Transpose => (y, x),
            Self::Rotate90 => (y, height - 1 - x),
            Self::Transverse => (height - 1 - y, width - 1 - x),
            Self::Rotate270 => (width - 1 - y, x),
        }
    }
}

/// Apply an orientation to an interleaved buffer with `channels` components per
/// pixel, returning the transformed buffer and its new dimensions.
///
/// Returns the input untouched when the buffer length does not match the stated
/// dimensions: a mismatch means an earlier stage is wrong, and silently
/// reshaping it would hide that.
#[must_use]
pub fn apply<T: Copy + Default>(
    data: &[T],
    width: u32,
    height: u32,
    channels: usize,
    orientation: Orientation,
) -> (Vec<T>, u32, u32) {
    let expected = width as usize * height as usize * channels;
    if data.len() != expected || width == 0 || height == 0 {
        return (data.to_vec(), width, height);
    }
    if orientation.is_identity() {
        return (data.to_vec(), width, height);
    }

    let (out_width, out_height) = if orientation.swaps_axes() {
        (height, width)
    } else {
        (width, height)
    };

    let mut out = vec![T::default(); expected];
    for y in 0..out_height {
        for x in 0..out_width {
            let (sx, sy) = orientation.source_of(x, y, width, height);
            if sx >= width || sy >= height {
                continue;
            }
            let source = (sy as usize * width as usize + sx as usize) * channels;
            let target = (y as usize * out_width as usize + x as usize) * channels;
            for channel in 0..channels {
                let value = data.get(source + channel).copied().unwrap_or_default();
                if let Some(slot) = out.get_mut(target + channel) {
                    *slot = value;
                }
            }
        }
    }
    (out, out_width, out_height)
}
