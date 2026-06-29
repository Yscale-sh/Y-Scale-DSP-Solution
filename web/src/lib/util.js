let _seq = 0
// Stable UI-only id for v-for keys (never sent to the server).
export function uid(prefix = 'id') {
  _seq += 1
  return `${prefix}-${_seq}-${Math.random().toString(36).slice(2, 7)}`
}
