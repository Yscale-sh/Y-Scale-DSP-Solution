// ── Y//SCALE DSP — client-side DSP math & helpers ───────────────────────────

// 30 ISO 1/3-octave band centers (Hz), IN ORDER (matches the server).
export const ISO_BANDS = [
  20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160,
  200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600,
  2000, 2500, 3150, 4000, 5000, 6300, 8000, 10000, 12500, 16000,
]

// Parametric band kinds. `usesGain` flags which expose a gain control.
export const BAND_KINDS = [
  { id: 'peaking', label: 'Peak', usesGain: true },
  { id: 'low_shelf', label: 'Low Shelf', usesGain: true },
  { id: 'high_shelf', label: 'High Shelf', usesGain: true },
  { id: 'low_pass', label: 'Low Pass', usesGain: false },
  { id: 'high_pass', label: 'High Pass', usesGain: false },
  { id: 'notch', label: 'Notch', usesGain: false },
  { id: 'band_pass', label: 'Band Pass', usesGain: false },
  { id: 'all_pass', label: 'All Pass', usesGain: false },
]

export const BAND_LABELS = Object.fromEntries(BAND_KINDS.map((k) => [k.id, k.label]))
export const bandUsesGain = (kind) => !!BAND_KINDS.find((k) => k.id === kind)?.usesGain

export const FREQ_MIN = 20
export const FREQ_MAX = 20000

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v))
export const lin2db = (v) => 20 * Math.log10(Math.max(v, 1e-6))
export const db2lin = (d) => Math.pow(10, d / 20)

// log-frequency <-> normalized 0..1 (for log x-axis mapping)
export const freqToNorm = (f) =>
  (Math.log10(clamp(f, FREQ_MIN, FREQ_MAX)) - Math.log10(FREQ_MIN)) /
  (Math.log10(FREQ_MAX) - Math.log10(FREQ_MIN))
export const normToFreq = (n) =>
  Math.pow(10, clamp(n, 0, 1) * (Math.log10(FREQ_MAX) - Math.log10(FREQ_MIN)) + Math.log10(FREQ_MIN))

export function fmtHz(f) {
  if (f >= 1000) {
    const k = f / 1000
    return (Number.isInteger(k) ? k.toFixed(0) : k.toFixed(k < 10 ? 2 : 1).replace(/\.?0+$/, '')) + 'k'
  }
  return Number.isInteger(f) ? `${f}` : `${f}`
}

export function fmtDb(d, digits = 1) {
  const sign = d > 0.05 ? '+' : ''
  return `${sign}${d.toFixed(digits)}`
}

// RBJ Audio-EQ-Cookbook biquad coefficients, normalized so a0 = 1.
// Returns [b0, b1, b2, a1, a2].
export function biquadCoeffs(kind, freq, q, gainDb, fs) {
  const A = Math.pow(10, gainDb / 40)
  const w0 = (2 * Math.PI * clamp(freq, 1, fs / 2 - 1)) / fs
  const cw = Math.cos(w0)
  const sw = Math.sin(w0)
  const alpha = sw / (2 * Math.max(q, 1e-3))
  let b0, b1, b2, a0, a1, a2

  switch (kind) {
    case 'peaking':
      b0 = 1 + alpha * A; b1 = -2 * cw; b2 = 1 - alpha * A
      a0 = 1 + alpha / A; a1 = -2 * cw; a2 = 1 - alpha / A
      break
    case 'low_shelf': {
      const s = 2 * Math.sqrt(A) * alpha
      b0 = A * ((A + 1) - (A - 1) * cw + s)
      b1 = 2 * A * ((A - 1) - (A + 1) * cw)
      b2 = A * ((A + 1) - (A - 1) * cw - s)
      a0 = (A + 1) + (A - 1) * cw + s
      a1 = -2 * ((A - 1) + (A + 1) * cw)
      a2 = (A + 1) + (A - 1) * cw - s
      break
    }
    case 'high_shelf': {
      const s = 2 * Math.sqrt(A) * alpha
      b0 = A * ((A + 1) + (A - 1) * cw + s)
      b1 = -2 * A * ((A - 1) + (A + 1) * cw)
      b2 = A * ((A + 1) + (A - 1) * cw - s)
      a0 = (A + 1) - (A - 1) * cw + s
      a1 = 2 * ((A - 1) - (A + 1) * cw)
      a2 = (A + 1) - (A - 1) * cw - s
      break
    }
    case 'low_pass':
      b0 = (1 - cw) / 2; b1 = 1 - cw; b2 = (1 - cw) / 2
      a0 = 1 + alpha; a1 = -2 * cw; a2 = 1 - alpha
      break
    case 'high_pass':
      b0 = (1 + cw) / 2; b1 = -(1 + cw); b2 = (1 + cw) / 2
      a0 = 1 + alpha; a1 = -2 * cw; a2 = 1 - alpha
      break
    case 'notch':
      b0 = 1; b1 = -2 * cw; b2 = 1
      a0 = 1 + alpha; a1 = -2 * cw; a2 = 1 - alpha
      break
    case 'band_pass':
      b0 = alpha; b1 = 0; b2 = -alpha
      a0 = 1 + alpha; a1 = -2 * cw; a2 = 1 - alpha
      break
    case 'all_pass':
      b0 = 1 - alpha; b1 = -2 * cw; b2 = 1 + alpha
      a0 = 1 + alpha; a1 = -2 * cw; a2 = 1 - alpha
      break
    default:
      b0 = 1; b1 = 0; b2 = 0; a0 = 1; a1 = 0; a2 = 0
  }
  return [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

// Magnitude (dB) of a normalized biquad at frequency f.
export function biquadMagDb(c, freq, fs) {
  const [b0, b1, b2, a1, a2] = c
  const w = (2 * Math.PI * freq) / fs
  const cw = Math.cos(w)
  const sw = Math.sin(w)
  const c2 = Math.cos(2 * w)
  const s2 = Math.sin(2 * w)
  const nRe = b0 + b1 * cw + b2 * c2
  const nIm = -(b1 * sw + b2 * s2)
  const dRe = 1 + a1 * cw + a2 * c2
  const dIm = -(a1 * sw + a2 * s2)
  const num = Math.hypot(nRe, nIm)
  const den = Math.hypot(dRe, dIm)
  return 20 * Math.log10(Math.max(num / den, 1e-7))
}

// Analog magnitude (dB) of a crossover at frequency f.
// Butterworth: -3 dB at fc, n*6 dB/oct. Linkwitz-Riley: cascade of two
// Butterworth(n/2), so -6 dB at fc with matching n*6 dB/oct asymptote.
export function crossoverMagDb(xover, freq) {
  if (!xover) return 0
  const fc = xover.freq
  const order = clamp(xover.order || 1, 1, 8)
  const ratio = xover.role === 'low_pass' ? freq / fc : fc / freq
  if (xover.kind === 'linkwitz_riley') {
    const nb = order / 2
    return 2 * (-10 * Math.log10(1 + Math.pow(ratio, 2 * nb)))
  }
  return -10 * Math.log10(1 + Math.pow(ratio, 2 * order))
}

// Linear interpolation of a 30-band graphic-EQ contribution at frequency f (dB).
export function graphicMagDb(gains, freq) {
  if (!gains || !gains.length) return 0
  if (freq <= ISO_BANDS[0]) return gains[0] || 0
  const last = ISO_BANDS.length - 1
  if (freq >= ISO_BANDS[last]) return gains[last] || 0
  for (let i = 0; i < last; i++) {
    const f0 = ISO_BANDS[i]
    const f1 = ISO_BANDS[i + 1]
    if (freq >= f0 && freq <= f1) {
      const t = (Math.log10(freq) - Math.log10(f0)) / (Math.log10(f1) - Math.log10(f0))
      return (gains[i] || 0) * (1 - t) + (gains[i + 1] || 0) * t
    }
  }
  return 0
}

// Combined channel magnitude response (dB) at frequency f: parametric EQ bands
// + optional crossover + optional graphic EQ.
export function combinedMagDb(freq, bands, fs, crossover, graphicEq) {
  let db = 0
  for (const band of bands) {
    const g = bandUsesGain(band.kind) ? band.gain_db : 0
    db += biquadMagDb(biquadCoeffs(band.kind, band.freq, band.q, g, fs), freq, fs)
  }
  if (crossover) db += crossoverMagDb(crossover, freq)
  if (graphicEq) db += graphicMagDb(graphicEq, freq)
  return db
}

// Sample the combined response over a log-frequency grid.
export function sampleResponse(bands, fs, opts = {}) {
  const { points = 280, crossover = null, graphicEq = null } = opts
  const out = new Array(points)
  for (let i = 0; i < points; i++) {
    const n = i / (points - 1)
    const f = normToFreq(n)
    out[i] = { f, norm: n, db: combinedMagDb(f, bands, fs, crossover, graphicEq) }
  }
  return out
}
