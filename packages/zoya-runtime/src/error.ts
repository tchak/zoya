export class $$ZoyaError extends Error {
  constructor(code: string, detail?: string) {
    super('$$zoya:' + code + (detail !== undefined ? ':' + detail : ''));
    this.name = '$$ZoyaError';
  }
}

export function $$throw(code: string, detail?: string): never {
  throw new $$ZoyaError(code, detail);
}
