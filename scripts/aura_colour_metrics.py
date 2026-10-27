"""Independent sRGB export measurements; distribution gaps do not imply scene equivalence."""
import io
from pathlib import Path

import numpy as np
from PIL import Image, ImageCms, ImageOps


def load_srgb(path):
    with Image.open(path) as opened:
        info = dict(format=opened.format, width=opened.width, height=opened.height,
                    embeddedIcc=bool(opened.info.get('icc_profile')),
                    exifOrientation=opened.getexif().get(274, 1))
        image = ImageOps.exif_transpose(opened)
        profile = opened.info.get('icc_profile')
        if profile:
            image = ImageCms.profileToProfile(image, ImageCms.ImageCmsProfile(io.BytesIO(profile)),
                                             ImageCms.createProfile('sRGB'), outputMode='RGB')
            info['interpretation'] = 'embedded ICC converted to sRGB'
        else:
            image = image.convert('RGB')
            if opened.info.get('srgb') is not None:
                info['srgbRenderingIntent'] = opened.info['srgb']
                info['interpretation'] = 'PNG sRGB chunk'
            else:
                info['interpretation'] = 'untagged RGB assumed sRGB'
        return np.asarray(image).copy(), info


def lab(rgb):
    srgb = rgb.astype(np.float64) / 255
    linear = np.where(srgb <= .04045, srgb / 12.92, ((srgb + .055) / 1.055) ** 2.4)
    xyz = linear @ np.array([[.4124564, .2126729, .0193339], [.3575761, .7151522, .119192],
                             [.1804375, .072175, .9503041]])
    xyz /= np.array([.95047, 1, 1.08883])
    f = np.where(xyz > (6 / 29) ** 3, np.cbrt(xyz), xyz / (3 * (6 / 29) ** 2) + 4 / 29)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]),
                     200 * (f[..., 1] - f[..., 2])], axis=-1)


def aligned_error(actual, expected):
    if actual.shape != expected.shape:
        raise ValueError(f'Aligned comparison requires identical dimensions: {actual.shape}, {expected.shape}')
    if np.array_equal(actual, expected):
        return dict(rgbMae=0., meanDeltaE76=0., p95DeltaE76=0., medianLightnessShift=0.,
                    meanCastShiftA=0., meanCastShiftB=0., exactPixels=True)
    delta = lab(actual) - lab(expected)
    de = np.linalg.norm(delta, axis=-1)
    return dict(rgbMae=float(np.abs(actual.astype(float) - expected.astype(float)).mean()),
                meanDeltaE76=float(de.mean()), p95DeltaE76=float(np.quantile(de, .95)),
                medianLightnessShift=float(np.median(delta[..., 0])),
                meanCastShiftA=float(delta[..., 1].mean()), meanCastShiftB=float(delta[..., 2].mean()),
                exactPixels=bool(np.array_equal(actual, expected)))


def descriptor(rgb):
    # Fixed-size analysis bounds memory and gives every image one normalized distribution.
    image = Image.fromarray(rgb)
    image.thumbnail((512, 512), Image.Resampling.LANCZOS)
    values = lab(np.asarray(image)).reshape(-1, 3)
    chroma = np.linalg.norm(values[:, 1:], axis=1)
    quantiles = [.05, .25, .5, .75, .95]
    neutral = (chroma <= 12) & (values[:, 0] >= 20) & (values[:, 0] <= 90)
    histogram = np.histogram2d(values[:, 1], values[:, 2], bins=32, range=[[-128, 128], [-128, 128]])[0]
    histogram = histogram / histogram.sum()
    return dict(lightnessQuantiles=np.quantile(values[:, 0], quantiles).tolist(),
                chromaQuantiles=np.quantile(chroma, quantiles).tolist(),
                meanLab=values.mean(axis=0).tolist(), medianLab=np.median(values, axis=0).tolist(),
                nearNeutralFraction=float(neutral.mean()),
                nearNeutralMedianAb=np.median(values[neutral, 1:], axis=0).tolist() if neutral.sum() >= 64 else None,
                chromaHistogram=histogram.reshape(-1).tolist())


def distribution_gap(actual, reference):
    p = np.asarray(actual['chromaHistogram'])
    q = np.asarray(reference['chromaHistogram'])
    mixture = (p + q) / 2
    def kl(first):
        used = first > 0
        return np.sum(first[used] * np.log2(first[used] / mixture[used]))
    neutral_a, neutral_b = actual['nearNeutralMedianAb'], reference['nearNeutralMedianAb']
    return dict(toneQuantileMaeL=float(np.abs(np.asarray(actual['lightnessQuantiles']) - reference['lightnessQuantiles']).mean()),
                chromaQuantileMae=float(np.abs(np.asarray(actual['chromaQuantiles']) - reference['chromaQuantiles']).mean()),
                paletteJensenShannonDistance=float(np.sqrt(max(0, (kl(p) + kl(q)) / 2))),
                nearNeutralCastGapAb=float(np.linalg.norm(np.asarray(neutral_a) - neutral_b)) if neutral_a is not None and neutral_b is not None else None,
                calibratedWhiteBalance='unverified: no corresponding known-neutral target or original WB recipe')


def public_descriptor(value):
    return {key: data for key, data in value.items() if key != 'chromaHistogram'}
