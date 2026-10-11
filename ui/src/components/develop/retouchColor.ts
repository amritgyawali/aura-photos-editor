/** Display sRGB; the native renderer converts it to the photograph's working space. */
export function colorRgb(hex: string): [number, number, number] {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) return [0, 0, 0];
  return [1,3,5].map(i => parseInt(hex.slice(i,i+2),16)/255) as [number,number,number];
}
export function colorHex(rgb: [number, number, number]): string {
  return '#' + rgb.map(v => Math.round(Math.max(0,Math.min(1,v))*255).toString(16).padStart(2,'0')).join('');
}
