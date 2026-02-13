class $$ZoyaError extends Error {
  constructor(code, detail) {
    super('$$zoya:' + code + (detail !== undefined ? ':' + detail : ''));
    this.name = '$$ZoyaError';
  }
}
function $$throw(code, detail) { throw new $$ZoyaError(code, detail); }
function $$is_obj(x) {
  return typeof x === 'object' && x !== null && !Array.isArray(x);
}
function $$div(a, b) {
  if (b === 0) $$throw("PANIC", "division by zero");
  return Math.trunc(a / b);
}
function $$div_bigint(a, b) {
  if (b === 0n) $$throw("PANIC", "division by zero");
  return a / b;
}
function $$mod(a, b) {
  if (b === 0) $$throw("PANIC", "modulo by zero");
  return a % b;
}
function $$mod_bigint(a, b) {
  if (b === 0n) $$throw("PANIC", "modulo by zero");
  return a % b;
}
function $$pow(a, b) {
  if (b < 0) $$throw("PANIC", "negative exponent");
  return a ** b;
}
function $$pow_bigint(a, b) {
  if (b < 0n) $$throw("PANIC", "negative exponent");
  return a ** b;
}
function $$eq(a, b) {
  if (a === b) return true;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
      if (!$$eq(a[i], b[i])) return false;
    }
    return true;
  }
  if ($$is_obj(a) && $$is_obj(b)) {
    if (a.$$hamt === true && b.$$hamt === true) {
      if (a.size !== b.size) return false;
      const ea = $$Dict.entries(a);
      for (let i = 0; i < ea.length; i++) {
        const v = $$Dict.get(b, ea[i][0]);
        if (v.$tag === "None" || !$$eq(ea[i][1], v.$0)) return false;
      }
      return true;
    }
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (let k of ka) {
      if (!$$eq(a[k], b[k])) return false;
    }
    return true;
  }
  return a === b;
}
function $$list_idx(arr, i) {
  const idx = i < 0 ? arr.length + i : i;
  return idx >= 0 && idx < arr.length ? { $tag: "Some", $0: arr[idx] } : { $tag: "None" };
}
function $$json_to_zoya(v) {
  if (v === null) return { $tag: "Null" };
  if (typeof v === "boolean") return { $tag: "Bool", $0: v };
  if (typeof v === "number") return Number.isInteger(v)
    ? { $tag: "Number", $0: { $tag: "Int", $0: v } }
    : { $tag: "Number", $0: { $tag: "Float", $0: v } };
  if (typeof v === "string") return { $tag: "String", $0: v };
  if (Array.isArray(v)) return { $tag: "Array", $0: v.map($$json_to_zoya) };
  return { $tag: "Object", $0: Object.entries(v).map(([k, val]) => [k, $$json_to_zoya(val)]) };
}