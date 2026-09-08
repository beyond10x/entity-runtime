// Hostile inputs: each keeps a parser's offset from advancing on image-size 2.0.2.
const be32 = (n) => [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255];
const ascii = (s) => [...s].map((c) => c.charCodeAt(0));
export const icnsZeroEntry = new Uint8Array([...ascii("icns"), ...be32(32), ...ascii("ic07"), ...be32(0), ...new Array(16).fill(0)]);
export const icnsHonest = new Uint8Array([...ascii("icns"), ...be32(16), ...ascii("ic07"), ...be32(8)]);
// meta(48) > iprp(36) > ipco(28) > ispe with size 0 followed by 12 payload bytes, so the width and
// height reads succeed and only the offset fails to advance.
export const heifZeroIspe = new Uint8Array([
  ...be32(16), ...ascii("ftyp"), ...ascii("heic"), ...be32(0),
  ...be32(48), ...ascii("meta"), ...be32(0),
  ...be32(36), ...ascii("iprp"),
  ...be32(28), ...ascii("ipco"),
  ...be32(0), ...ascii("ispe"), ...be32(0), ...be32(64), ...be32(64),
]);
export const heifHonest = new Uint8Array([
  ...be32(16), ...ascii("ftyp"), ...ascii("heic"), ...be32(0),
  ...be32(48), ...ascii("meta"), ...be32(0),
  ...be32(36), ...ascii("iprp"),
  ...be32(28), ...ascii("ipco"),
  ...be32(20), ...ascii("ispe"), ...be32(0), ...be32(64), ...be32(48),
]);
export const jxlZeroJxlp = new Uint8Array([
  ...be32(12), ...ascii("JXL "), 0x0d, 0x0a, 0x87, 0x0a,
  ...be32(20), ...ascii("ftyp"), ...ascii("jxl "), ...be32(0), ...ascii("jxl "),
  ...be32(0), ...ascii("jxlp"), ...be32(0),
]);
