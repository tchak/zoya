import { $$ZoyaError, $$throw } from './error';
import { $$eq, $$is_obj } from './equality';
import {
  $$div,
  $$div_bigint,
  $$mod,
  $$mod_bigint,
  $$pow,
  $$pow_bigint,
} from './arithmetic';
import { $$list_idx } from './list-idx';
import { $$json_to_zoya, $$zoya_to_json } from './json';
import { $$zoya_to_js, $$js_to_zoya, $$run } from './zoya';
import { $$Dict } from './hamt';
import { $$Set } from './set';
import { $$Int } from './int';
import { $$BigInt } from './bigint';
import { $$Float } from './float';
import { $$String } from './string';
import { $$List } from './list';
import { $$Task } from './task';

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
  $$run,
  $$Dict,
  $$Set,
  $$Int,
  $$BigInt,
  $$Float,
  $$String,
  $$List,
  $$Task,
});
