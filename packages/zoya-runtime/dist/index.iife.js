(function() {


//#region src/error.ts
	var $$ZoyaError = class extends Error {
		constructor(code, detail) {
			super("$$zoya:" + code + (detail !== void 0 ? ":" + detail : ""));
			this.name = "$$ZoyaError";
		}
	};
	function $$throw(code, detail) {
		throw new $$ZoyaError(code, detail);
	}

//#endregion
//#region src/hamt.ts
	function $$hash(v) {
		if (typeof v === "number") return v | 0;
		if (typeof v === "bigint") return Number(v & 2147483647n) | 0;
		if (typeof v === "boolean") return v ? 1 : 0;
		if (typeof v === "string") {
			let h = 0;
			for (let i = 0; i < v.length; i++) h = Math.imul(31, h) + v.charCodeAt(i) | 0;
			return h;
		}
		if (Array.isArray(v)) {
			let h = 1;
			for (let i = 0; i < v.length; i++) h = Math.imul(31, h) + $$hash(v[i]) | 0;
			return h;
		}
		if (typeof v === "object" && v !== null) {
			let h = 1;
			const keys = Object.keys(v).sort();
			for (let i = 0; i < keys.length; i++) {
				h = Math.imul(31, h) + $$hash(keys[i]) | 0;
				h = Math.imul(31, h) + $$hash(v[keys[i]]) | 0;
			}
			return h;
		}
		return 0;
	}
	const BITS = 5;
	const MASK = (1 << BITS) - 1;
	function popcount(x) {
		x = x - (x >> 1 & 1431655765);
		x = (x & 858993459) + (x >> 2 & 858993459);
		return (x + (x >> 4) & 252645135) * 16843009 >> 24;
	}
	const EMPTY$1 = Object.freeze({
		$$hamt: true,
		bitmap: 0,
		children: Object.freeze([]),
		size: 0
	});
	function leaf(hash, key, value) {
		return Object.freeze({
			$$hamt: true,
			hash,
			key,
			value,
			size: 1
		});
	}
	function collision(hash, bucket) {
		return Object.freeze({
			$$hamt: true,
			hash,
			bucket: Object.freeze(bucket),
			size: bucket.length
		});
	}
	function internal(bitmap, children, size) {
		return Object.freeze({
			$$hamt: true,
			bitmap,
			children: Object.freeze(children),
			size
		});
	}
	function isLeaf(node) {
		return "key" in node;
	}
	function isCollision(node) {
		return "bucket" in node;
	}
	function isInternal(node) {
		return "bitmap" in node && !("bucket" in node) && !("key" in node);
	}
	function getIndex(bitmap, bit) {
		return popcount(bitmap & bit - 1);
	}
	function get(node, hash, key, shift) {
		if (node === EMPTY$1) return void 0;
		if (isLeaf(node)) return node.hash === hash && $$eq(node.key, key) ? node.value : void 0;
		if (isCollision(node)) {
			if (node.hash !== hash) return void 0;
			for (let i = 0; i < node.bucket.length; i++) if ($$eq(node.bucket[i].key, key)) return node.bucket[i].value;
			return;
		}
		const bit = 1 << (hash >>> shift & MASK);
		if ((node.bitmap & bit) === 0) return void 0;
		const idx = getIndex(node.bitmap, bit);
		return get(node.children[idx], hash, key, shift + BITS);
	}
	function insert(node, hash, key, value, shift) {
		if (node === EMPTY$1) return leaf(hash, key, value);
		if (isLeaf(node)) {
			if (node.hash === hash) {
				if ($$eq(node.key, key)) return leaf(hash, key, value);
				return collision(hash, [{
					key: node.key,
					value: node.value
				}, {
					key,
					value
				}]);
			}
			return mergeLeaves(shift, node, leaf(hash, key, value));
		}
		if (isCollision(node)) {
			if (node.hash === hash) {
				const newBucket = node.bucket.slice();
				for (let i = 0; i < newBucket.length; i++) if ($$eq(newBucket[i].key, key)) {
					newBucket[i] = {
						key,
						value
					};
					return collision(hash, newBucket);
				}
				newBucket.push({
					key,
					value
				});
				return collision(hash, newBucket);
			}
			return insert(internal(1 << (node.hash >>> shift & MASK), [node], node.size), hash, key, value, shift);
		}
		const bit = 1 << (hash >>> shift & MASK);
		const idx = getIndex(node.bitmap, bit);
		if ((node.bitmap & bit) === 0) {
			const newChildren = node.children.slice();
			newChildren.splice(idx, 0, leaf(hash, key, value));
			return internal(node.bitmap | bit, newChildren, node.size + 1);
		}
		const child = node.children[idx];
		const newChild = insert(child, hash, key, value, shift + BITS);
		if (newChild === child) return node;
		const newChildren = node.children.slice();
		newChildren[idx] = newChild;
		return internal(node.bitmap, newChildren, node.size - child.size + newChild.size);
	}
	function mergeLeaves(shift, a, b) {
		const fragA = a.hash >>> shift & MASK;
		const fragB = b.hash >>> shift & MASK;
		if (fragA === fragB) {
			const child = mergeLeaves(shift + BITS, a, b);
			return internal(1 << fragA, [child], a.size + b.size);
		}
		return internal(1 << fragA | 1 << fragB, fragA < fragB ? [a, b] : [b, a], a.size + b.size);
	}
	function remove(node, hash, key, shift) {
		if (node === EMPTY$1) return EMPTY$1;
		if (isLeaf(node)) return node.hash === hash && $$eq(node.key, key) ? EMPTY$1 : node;
		if (isCollision(node)) {
			if (node.hash !== hash) return node;
			const newBucket = node.bucket.filter((e) => !$$eq(e.key, key));
			if (newBucket.length === node.bucket.length) return node;
			if (newBucket.length === 1) return leaf(hash, newBucket[0].key, newBucket[0].value);
			return collision(hash, newBucket);
		}
		const bit = 1 << (hash >>> shift & MASK);
		if ((node.bitmap & bit) === 0) return node;
		const idx = getIndex(node.bitmap, bit);
		const child = node.children[idx];
		const newChild = remove(child, hash, key, shift + BITS);
		if (newChild === child) return node;
		if (newChild === EMPTY$1) {
			if (node.children.length === 1) return EMPTY$1;
			const newChildren = node.children.slice();
			newChildren.splice(idx, 1);
			const newInternal = internal(node.bitmap ^ bit, newChildren, node.size - child.size);
			if (newInternal.children.length === 1 && !isInternal(newInternal.children[0])) return newInternal.children[0];
			return newInternal;
		}
		const newChildren = node.children.slice();
		newChildren[idx] = newChild;
		return internal(node.bitmap, newChildren, node.size - child.size + newChild.size);
	}
	function collect(node, fn) {
		if (node === EMPTY$1) return;
		if (isLeaf(node)) {
			fn(node.key, node.value);
			return;
		}
		if (isCollision(node)) {
			for (let i = 0; i < node.bucket.length; i++) fn(node.bucket[i].key, node.bucket[i].value);
			return;
		}
		for (let i = 0; i < node.children.length; i++) collect(node.children[i], fn);
	}
	function keys(node) {
		const result = [];
		collect(node, (k) => {
			result.push(k);
		});
		return result;
	}
	function values(node) {
		const result = [];
		collect(node, (_k, v) => {
			result.push(v);
		});
		return result;
	}
	function entries(node) {
		const result = [];
		collect(node, (k, v) => {
			result.push([k, v]);
		});
		return result;
	}
	const $$Dict = {
		empty() {
			return EMPTY$1;
		},
		get(d, k) {
			const v = get(d, $$hash(k), k, 0);
			return v === void 0 ? { $tag: "None" } : {
				$tag: "Some",
				$0: v
			};
		},
		insert(d, k, v) {
			return insert(d, $$hash(k), k, v, 0);
		},
		remove(d, k) {
			return remove(d, $$hash(k), k, 0);
		},
		keys,
		values,
		len(d) {
			return d.size;
		},
		has(d, k) {
			return get(d, $$hash(k), k, 0) !== void 0;
		},
		from(pairs) {
			let d = EMPTY$1;
			for (let i = 0; i < pairs.length; i++) d = insert(d, $$hash(pairs[i][0]), pairs[i][0], pairs[i][1], 0);
			return d;
		},
		entries
	};

//#endregion
//#region src/equality.ts
	function $$is_obj(x) {
		return typeof x === "object" && x !== null && !Array.isArray(x);
	}
	function $$eq(a, b) {
		if (a === b) return true;
		if (a instanceof Uint8Array && b instanceof Uint8Array) {
			if (a.length !== b.length) return false;
			for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
			return true;
		}
		if (Array.isArray(a) && Array.isArray(b)) {
			if (a.length !== b.length) return false;
			for (let i = 0; i < a.length; i++) if (!$$eq(a[i], b[i])) return false;
			return true;
		}
		if ($$is_obj(a) && $$is_obj(b)) {
			if (a.$$set === true && b.$$set === true) {
				const aData = a.$$data;
				const bData = b.$$data;
				if ($$Dict.len(aData) !== $$Dict.len(bData)) return false;
				const ks = $$Dict.keys(aData);
				for (let j = 0; j < ks.length; j++) if (!$$Dict.has(bData, ks[j])) return false;
				return true;
			}
			if (a.$$hamt === true && b.$$hamt === true) {
				const aNode = a;
				const bNode = b;
				if (a.size !== b.size) return false;
				const ea = $$Dict.entries(aNode);
				for (let i = 0; i < ea.length; i++) {
					const v = $$Dict.get(bNode, ea[i][0]);
					if (v.$tag === "None" || !$$eq(ea[i][1], v.$0)) return false;
				}
				return true;
			}
			const ka = Object.keys(a), kb = Object.keys(b);
			if (ka.length !== kb.length) return false;
			for (const k of ka) if (!$$eq(a[k], b[k])) return false;
			return true;
		}
		return a === b;
	}

//#endregion
//#region src/arithmetic.ts
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

//#endregion
//#region src/list-idx.ts
	function $$list_idx(arr, i) {
		const idx = i < 0 ? arr.length + i : i;
		return idx >= 0 && idx < arr.length ? {
			$tag: "Some",
			$0: arr[idx]
		} : { $tag: "None" };
	}

//#endregion
//#region src/json.ts
	function $$json_to_zoya(v) {
		if (v === null) return { $tag: "Null" };
		if (typeof v === "boolean") return {
			$tag: "Bool",
			$0: v
		};
		if (typeof v === "number") return Number.isInteger(v) ? {
			$tag: "Number",
			$0: {
				$tag: "Int",
				$0: v
			}
		} : {
			$tag: "Number",
			$0: {
				$tag: "Float",
				$0: v
			}
		};
		if (typeof v === "string") return {
			$tag: "String",
			$0: v
		};
		if (Array.isArray(v)) return {
			$tag: "Array",
			$0: v.map($$json_to_zoya)
		};
		return {
			$tag: "Object",
			$0: $$Dict.from(Object.entries(v).map(([k, val]) => [k, $$json_to_zoya(val)]))
		};
	}
	function $$zoya_to_json(v) {
		switch (v.$tag) {
			case "Null": return null;
			case "Bool": return v.$0;
			case "Number": return v.$0.$0;
			case "String": return v.$0;
			case "Array": return v.$0.map($$zoya_to_json);
			case "Object": return Object.fromEntries($$Dict.entries(v.$0).map(([k, val]) => [k, $$zoya_to_json(val)]));
		}
	}

//#endregion
//#region src/set.ts
	const SENTINEL = true;
	const EMPTY = Object.freeze({
		$$set: true,
		$$data: $$Dict.empty()
	});
	function wrap(data) {
		return Object.freeze({
			$$set: true,
			$$data: data
		});
	}
	const $$Set = {
		empty() {
			return EMPTY;
		},
		contains(s, v) {
			return $$Dict.has(s.$$data, v);
		},
		insert(s, v) {
			return wrap($$Dict.insert(s.$$data, v, SENTINEL));
		},
		remove(s, v) {
			return wrap($$Dict.remove(s.$$data, v));
		},
		len(s) {
			return $$Dict.len(s.$$data);
		},
		to_list(s) {
			return $$Dict.keys(s.$$data);
		},
		is_disjoint(s, o) {
			const ks = $$Dict.keys(s.$$data);
			for (let i = 0; i < ks.length; i++) if ($$Dict.has(o.$$data, ks[i])) return false;
			return true;
		},
		is_subset(s, o) {
			const ks = $$Dict.keys(s.$$data);
			for (let i = 0; i < ks.length; i++) if (!$$Dict.has(o.$$data, ks[i])) return false;
			return true;
		},
		is_superset(s, o) {
			const ko = $$Dict.keys(o.$$data);
			for (let i = 0; i < ko.length; i++) if (!$$Dict.has(s.$$data, ko[i])) return false;
			return true;
		},
		difference(s, o) {
			const ks = $$Dict.keys(s.$$data);
			let d = s.$$data;
			for (let i = 0; i < ks.length; i++) if ($$Dict.has(o.$$data, ks[i])) d = $$Dict.remove(d, ks[i]);
			return wrap(d);
		},
		intersection(s, o) {
			let smaller, larger;
			if ($$Dict.len(s.$$data) <= $$Dict.len(o.$$data)) {
				smaller = s;
				larger = o;
			} else {
				smaller = o;
				larger = s;
			}
			const ks = $$Dict.keys(smaller.$$data);
			let d = $$Dict.empty();
			for (let i = 0; i < ks.length; i++) if ($$Dict.has(larger.$$data, ks[i])) d = $$Dict.insert(d, ks[i], SENTINEL);
			return wrap(d);
		},
		union(s, o) {
			const ko = $$Dict.keys(o.$$data);
			let d = s.$$data;
			for (let i = 0; i < ko.length; i++) d = $$Dict.insert(d, ko[i], SENTINEL);
			return wrap(d);
		},
		from(items) {
			let d = $$Dict.empty();
			for (let i = 0; i < items.length; i++) d = $$Dict.insert(d, items[i], SENTINEL);
			return wrap(d);
		}
	};

//#endregion
//#region src/task.ts
	const $$Task = {
		of(value) {
			return Object.freeze({
				$task: true,
				run: () => Promise.resolve(value)
			});
		},
		map(task, f) {
			return Object.freeze({
				$task: true,
				run: () => task.run().then(f)
			});
		},
		and_then(task, f) {
			return Object.freeze({
				$task: true,
				run: () => task.run().then((v) => f(v).run())
			});
		},
		all(tasks) {
			return Object.freeze({
				$task: true,
				run: () => Promise.all(tasks.map((t) => t.run()))
			});
		},
		tap(task, f) {
			return Object.freeze({
				$task: true,
				run: () => task.run().then((v) => {
					f(v);
					return v;
				})
			});
		},
		zip(a, b) {
			return Object.freeze({
				$task: true,
				run: () => Promise.all([a.run(), b.run()])
			});
		},
		zip3(a, b, c) {
			return Object.freeze({
				$task: true,
				run: () => Promise.all([
					a.run(),
					b.run(),
					c.run()
				])
			});
		},
		zip4(a, b, c, d) {
			return Object.freeze({
				$task: true,
				run: () => Promise.all([
					a.run(),
					b.run(),
					c.run(),
					d.run()
				])
			});
		},
		delay(ms) {
			return Object.freeze({
				$task: true,
				run: () => new Promise((r) => setTimeout(() => r([]), ms))
			});
		}
	};

//#endregion
//#region src/zoya.ts
	function valueDataToZoya(data) {
		if (data === "Unit") return {};
		if ("Tuple" in data) {
			const out = {};
			for (let i = 0; i < data.Tuple.length; i++) out[i] = $$value_to_zoya(data.Tuple[i]);
			return out;
		}
		const out = {};
		const keys = Object.keys(data.Struct);
		for (let i = 0; i < keys.length; i++) out[keys[i]] = $$value_to_zoya(data.Struct[keys[i]]);
		return out;
	}
	function $$value_to_zoya(v) {
		if ("Int" in v) return v.Int;
		if ("BigInt" in v) return globalThis.BigInt(v.BigInt);
		if ("Float" in v) return v.Float;
		if ("Bool" in v) return v.Bool;
		if ("String" in v) return v.String;
		if ("List" in v) return v.List.map($$value_to_zoya);
		if ("Tuple" in v) return v.Tuple.map($$value_to_zoya);
		if ("Set" in v) return $$Set.from(v.Set.map($$value_to_zoya));
		if ("Dict" in v) return $$Dict.from(v.Dict.map(([k, val]) => [$$value_to_zoya(k), $$value_to_zoya(val)]));
		if ("Struct" in v) {
			const obj = { $tag: v.Struct.name };
			Object.assign(obj, valueDataToZoya(v.Struct.data));
			return obj;
		}
		if ("EnumVariant" in v) {
			const obj = { $tag: v.EnumVariant.variant_name };
			Object.assign(obj, valueDataToZoya(v.EnumVariant.data));
			return obj;
		}
		if ("Task" in v) return $$Task.of($$value_to_zoya(v.Task));
		if ("Bytes" in v) return new Uint8Array(v.Bytes);
		$$throw("PANIC", `unexpected value in $$value_to_zoya: ${JSON.stringify(v)}`);
	}
	async function $$zoya_to_js(v) {
		if (v === null || v === void 0) $$throw("PANIC", `unexpected ${v} in $$zoya_to_js`);
		if (typeof v === "function") $$throw("PANIC", "unexpected function in $$zoya_to_js");
		if (typeof v === "boolean" || typeof v === "number" || typeof v === "string" || typeof v === "bigint") return v;
		if (v instanceof Uint8Array) return v;
		if (Array.isArray(v)) {
			const result = [];
			for (let i = 0; i < v.length; i++) result.push(await $$zoya_to_js(v[i]));
			const tagged = v;
			if (tagged.$tag) result.$tag = tagged.$tag;
			return result;
		}
		if (typeof v === "object") {
			const obj = v;
			if (obj.$task === true) {
				const run = obj.run;
				const arr = [await $$zoya_to_js(await run())];
				arr.$tag = "Task";
				return arr;
			}
			if (obj.$$set === true) {
				const keys = $$Dict.keys(obj.$$data);
				const result = [];
				for (let i = 0; i < keys.length; i++) result.push(await $$zoya_to_js(keys[i]));
				result.$tag = "Set";
				return result;
			}
			if (obj.$$hamt === true) {
				const entries = $$Dict.entries(v);
				const result = [];
				for (let i = 0; i < entries.length; i++) result.push([await $$zoya_to_js(entries[i][0]), await $$zoya_to_js(entries[i][1])]);
				result.$tag = "Dict";
				return result;
			}
			const out = {};
			const keys = Object.keys(obj);
			for (let i = 0; i < keys.length; i++) out[keys[i]] = await $$zoya_to_js(obj[keys[i]]);
			return out;
		}
		$$throw("PANIC", `unexpected value in $$zoya_to_js: ${typeof v}`);
	}
	function $$js_to_zoya(v) {
		if (v === null || v === void 0) $$throw("PANIC", `unexpected ${v} in $$js_to_zoya`);
		if (typeof v === "function") $$throw("PANIC", "unexpected function in $$js_to_zoya");
		if (typeof v === "boolean" || typeof v === "number" || typeof v === "string" || typeof v === "bigint") return v;
		if (v instanceof Uint8Array) return v;
		if (Array.isArray(v)) {
			const tagged = v;
			if (tagged.$tag === "Task") return $$Task.of($$js_to_zoya(v[0]));
			if (tagged.$tag === "Set") return $$Set.from(v.map($$js_to_zoya));
			if (tagged.$tag === "Dict") return $$Dict.from(v.map((e) => {
				const pair = e;
				return [$$js_to_zoya(pair[0]), $$js_to_zoya(pair[1])];
			}));
			return v.map($$js_to_zoya);
		}
		if (typeof v === "object") {
			const obj = v;
			const out = {};
			const keys = Object.keys(obj);
			for (let i = 0; i < keys.length; i++) out[keys[i]] = $$js_to_zoya(obj[keys[i]]);
			return out;
		}
		$$throw("PANIC", `unexpected value in $$js_to_zoya: ${typeof v}`);
	}
	const $$jobs = [];
	function $$enqueue(job) {
		$$jobs.push(job);
		return [];
	}
	async function $$run(qualified_path, ...args) {
		const js_name = "$" + qualified_path.replace(/::/g, "$");
		const fn = globalThis[js_name];
		if (typeof fn !== "function") $$throw("PANIC", `function not found: ${qualified_path}`);
		if (fn.length !== args.length) $$throw("PANIC", `arity mismatch for ${qualified_path}: expected ${fn.length} arguments, got ${args.length}`);
		const result = fn(...args.map($$js_to_zoya));
		const collected = $$jobs.splice(0);
		return {
			value: await $$zoya_to_js(result),
			jobs: await Promise.all(collected.map($$zoya_to_js))
		};
	}

//#endregion
//#region src/int.ts
	const $$Int = {
		abs(x) {
			return Math.abs(x);
		},
		to_string(x) {
			return String(x);
		},
		to_float(x) {
			return x;
		},
		min(x, y) {
			return Math.min(x, y);
		},
		max(x, y) {
			return Math.max(x, y);
		},
		pow(x, y) {
			return Math.pow(x, y);
		},
		clamp(x, min, max) {
			return Math.min(Math.max(x, min), max);
		},
		signum(x) {
			return Math.sign(x);
		},
		is_positive(x) {
			return x > 0;
		},
		is_negative(x) {
			return x < 0;
		},
		is_zero(x) {
			return x === 0;
		},
		to_bigint(x) {
			return BigInt(x);
		}
	};

//#endregion
//#region src/bigint.ts
	const $$BigInt = {
		abs(x) {
			return x < 0n ? -x : x;
		},
		to_string(x) {
			return String(x);
		},
		min(x, y) {
			return x < y ? x : y;
		},
		max(x, y) {
			return x > y ? x : y;
		},
		pow(x, y) {
			return x ** y;
		},
		clamp(x, min, max) {
			return x < min ? min : x > max ? max : x;
		},
		signum(x) {
			return x < 0n ? -1n : x > 0n ? 1n : 0n;
		},
		is_positive(x) {
			return x > 0n;
		},
		is_negative(x) {
			return x < 0n;
		},
		is_zero(x) {
			return x === 0n;
		},
		to_int(x) {
			return Number(x);
		}
	};

//#endregion
//#region src/float.ts
	const $$Float = {
		abs(x) {
			return Math.abs(x);
		},
		to_string(x) {
			return String(x);
		},
		to_int(x) {
			return Math.trunc(x);
		},
		floor(x) {
			return Math.floor(x);
		},
		ceil(x) {
			return Math.ceil(x);
		},
		round(x) {
			return Math.round(x);
		},
		sqrt(x) {
			return Math.sqrt(x);
		},
		min(x, y) {
			return Math.min(x, y);
		},
		max(x, y) {
			return Math.max(x, y);
		},
		pow(x, y) {
			return Math.pow(x, y);
		},
		clamp(x, min, max) {
			return Math.min(Math.max(x, min), max);
		},
		signum(x) {
			return Math.sign(x);
		},
		is_positive(x) {
			return x > 0;
		},
		is_negative(x) {
			return x < 0;
		},
		is_zero(x) {
			return x === 0;
		}
	};

//#endregion
//#region src/string.ts
	const $$String = {
		len(s) {
			return s.length;
		},
		contains(s, needle) {
			return s.includes(needle);
		},
		starts_with(s, prefix) {
			return s.startsWith(prefix);
		},
		ends_with(s, suffix) {
			return s.endsWith(suffix);
		},
		to_uppercase(s) {
			return s.toUpperCase();
		},
		to_lowercase(s) {
			return s.toLowerCase();
		},
		trim(s) {
			return s.trim();
		},
		trim_start(s) {
			return s.trimStart();
		},
		trim_end(s) {
			return s.trimEnd();
		},
		replace(s, from, to) {
			return s.replaceAll(from, to);
		},
		repeat(s, n) {
			return s.repeat(n);
		},
		split(s, sep) {
			return s.split(sep);
		},
		chars(s) {
			return Array.from(s);
		},
		find(s, needle) {
			const i = s.indexOf(needle);
			return i < 0 ? { $tag: "None" } : {
				$tag: "Some",
				$0: i
			};
		},
		slice(s, start, end) {
			return s.slice(start, end);
		},
		reverse(s) {
			return [...s].reverse().join("");
		},
		replace_first(s, from, to) {
			return s.replace(from, to);
		},
		pad_start(s, len, fill) {
			return s.padStart(len, fill);
		},
		pad_end(s, len, fill) {
			return s.padEnd(len, fill);
		},
		to_int(s) {
			const n = parseInt(s, 10);
			return isNaN(n) ? { $tag: "None" } : {
				$tag: "Some",
				$0: n
			};
		},
		to_float(s) {
			const n = parseFloat(s);
			return isNaN(n) ? { $tag: "None" } : {
				$tag: "Some",
				$0: n
			};
		}
	};

//#endregion
//#region src/list.ts
	const $$List = {
		len(xs) {
			return xs.length;
		},
		reverse(xs) {
			return [...xs].reverse();
		},
		push(xs, item) {
			return [...xs, item];
		},
		map(xs, f) {
			return xs.map(f);
		},
		filter(xs, f) {
			return xs.filter(f);
		},
		fold(xs, init, f) {
			return xs.reduce(f, init);
		},
		filter_map(xs, f) {
			const r = [];
			for (let i = 0; i < xs.length; i++) {
				const v = f(xs[i]);
				if (v.$tag === "Some") r.push(v.$0);
			}
			return r;
		},
		truncate(xs, len) {
			return xs.slice(0, len);
		},
		insert(xs, index, value) {
			return [
				...xs.slice(0, index),
				value,
				...xs.slice(index)
			];
		},
		remove(xs, index) {
			return [...xs.slice(0, index), ...xs.slice(index + 1)];
		}
	};

//#endregion
//#region src/bytes.ts
	function encodeUTF8(s) {
		const bytes = [];
		for (let i = 0; i < s.length; i++) {
			let c = s.charCodeAt(i);
			if (c >= 55296 && c <= 56319 && i + 1 < s.length) {
				const lo = s.charCodeAt(i + 1);
				if (lo >= 56320 && lo <= 57343) {
					c = (c - 55296 << 10) + (lo - 56320) + 65536;
					i++;
				}
			}
			if (c < 128) bytes.push(c);
			else if (c < 2048) bytes.push(192 | c >> 6, 128 | c & 63);
			else if (c < 65536) bytes.push(224 | c >> 12, 128 | c >> 6 & 63, 128 | c & 63);
			else bytes.push(240 | c >> 18, 128 | c >> 12 & 63, 128 | c >> 6 & 63, 128 | c & 63);
		}
		return new Uint8Array(bytes);
	}
	function decodeUTF8(b) {
		let s = "";
		let i = 0;
		while (i < b.length) {
			const byte = b[i];
			let cp;
			if (byte < 128) {
				cp = byte;
				i++;
			} else if ((byte & 224) === 192) {
				cp = (byte & 31) << 6 | b[i + 1] & 63;
				i += 2;
			} else if ((byte & 240) === 224) {
				cp = (byte & 15) << 12 | (b[i + 1] & 63) << 6 | b[i + 2] & 63;
				i += 3;
			} else {
				cp = (byte & 7) << 18 | (b[i + 1] & 63) << 12 | (b[i + 2] & 63) << 6 | b[i + 3] & 63;
				i += 4;
			}
			if (cp < 65536) s += String.fromCharCode(cp);
			else {
				cp -= 65536;
				s += String.fromCharCode(55296 + (cp >> 10), 56320 + (cp & 1023));
			}
		}
		return s;
	}
	const $$Bytes = {
		len(b) {
			return b.length;
		},
		get(b, index) {
			if (index < 0 || index >= b.length) return { $tag: "None" };
			return {
				$tag: "Some",
				$0: b[index]
			};
		},
		slice(b, start, end) {
			return b.slice(start, end);
		},
		concat(a, b) {
			const result = new Uint8Array(a.length + b.length);
			result.set(a);
			result.set(b, a.length);
			return result;
		},
		to_list(b) {
			return Array.from(b);
		},
		from_list(list) {
			return new Uint8Array(list);
		},
		to_string(b) {
			return decodeUTF8(b);
		},
		from_string(s) {
			return encodeUTF8(s);
		}
	};

//#endregion
//#region src/index.ts
	Object.assign(globalThis, {
		$$ZoyaError,
		$$throw,
		$$eq,
		$$is_obj,
		$$div,
		$$div_bigint,
		$$mod,
		$$mod_bigint,
		$$pow,
		$$pow_bigint,
		$$list_idx,
		$$json_to_zoya,
		$$zoya_to_json,
		$$zoya_to_js,
		$$js_to_zoya,
		$$value_to_zoya,
		$$run,
		$$enqueue,
		$$Dict,
		$$Set,
		$$Int,
		$$BigInt,
		$$Float,
		$$String,
		$$List,
		$$Task,
		$$Bytes
	});

//#endregion
})();