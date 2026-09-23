import type { RenderDto } from '../../ipc/types';

/** Wrap the backend's interleaved RGB in a lossless browser-readable BMP container. */
export function rgbDataUrl(render: RenderDto): string | null {
  const { width, height } = render;
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1) return null;
  const rgb = atob(render.rgbBase64);
  if (rgb.length !== width * height * 3) return null;
  const stride = Math.ceil(width * 3 / 4) * 4;
  const bytes = new Uint8Array(54 + stride * height);
  const header = new DataView(bytes.buffer);
  bytes[0] = 66; bytes[1] = 77;
  header.setUint32(2, bytes.length, true);
  header.setUint32(10, 54, true);
  header.setUint32(14, 40, true);
  header.setInt32(18, width, true);
  header.setInt32(22, -height, true); // top-down rows, same as the renderer
  header.setUint16(26, 1, true);
  header.setUint16(28, 24, true);
  header.setUint32(34, stride * height, true);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const from = (y * width + x) * 3;
      const to = 54 + y * stride + x * 3;
      bytes[to] = rgb.charCodeAt(from + 2);
      bytes[to + 1] = rgb.charCodeAt(from + 1);
      bytes[to + 2] = rgb.charCodeAt(from);
    }
  }
  const chunks: string[] = [];
  for (let at = 0; at < bytes.length; at += 8192) {
    chunks.push(String.fromCharCode(...bytes.subarray(at, at + 8192)));
  }
  return `data:image/bmp;base64,${btoa(chunks.join(''))}`;
}
