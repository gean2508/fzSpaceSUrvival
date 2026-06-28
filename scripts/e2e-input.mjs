/** Valid input payload for e2e scripts (matches InputMessageSchema). */
export function makeInput(seq, overrides = {}) {
  return {
    seq,
    moveX: 0,
    moveZ: 0,
    aimX: 0,
    aimZ: 1,
    abilities: 0,
    sprint: 0,
    action: 0,
    ...overrides,
  };
}
