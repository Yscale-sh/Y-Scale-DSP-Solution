<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue'
import { ISO_BANDS, fmtHz, clamp } from '../lib/dsp.js'

const props = defineProps({
  spectrum: { type: Array, default: () => [] }, // per-band dBFS (30 ISO bands)
})

const N = 30
const FLOOR = -78
const CEIL = 6
const LABELS = [0, 4, 7, 10, 14, 17, 20, 24, 27, 29] // ISO indices to annotate

const reduced =
  typeof window !== 'undefined' && window.matchMedia
    ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
    : false

const disp = ref(new Array(N).fill(0)) // smoothed 0..1
const peak = ref(new Array(N).fill(0)) // held peak 0..1
let hold = new Array(N).fill(0)
let raf = 0
let lastT = 0

const norm = (db) => clamp((db - FLOOR) / (CEIL - FLOOR), 0, 1)

function tick(t) {
  raf = requestAnimationFrame(tick)
  const dt = Math.min(64, t - lastT || 16)
  lastT = t
  const d = disp.value.slice()
  const p = peak.value.slice()
  for (let i = 0; i < N; i++) {
    const target = norm(props.spectrum[i] ?? FLOOR)
    // snappy attack, gentle release
    const a = target > d[i] ? 0.5 : 1 - Math.exp(-dt / 240)
    d[i] += (target - d[i]) * a
    if (d[i] >= p[i]) {
      p[i] = d[i]
      hold[i] = 700
    } else if (hold[i] > 0) {
      hold[i] -= dt
    } else {
      p[i] = Math.max(0, p[i] - dt * 0.0006)
    }
  }
  disp.value = d
  peak.value = p
}

onMounted(() => {
  if (reduced) {
    disp.value = props.spectrum.map(norm)
    return
  }
  raf = requestAnimationFrame(tick)
})
onBeforeUnmount(() => cancelAnimationFrame(raf))

const isLabel = (i) => LABELS.includes(i)
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-cool); animation-delay: 30ms">
    <header class="flex items-center justify-between mb-4">
      <div>
        <p class="eyebrow">Output · Real-Time Analyzer</p>
        <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">SPECTRUM · RTA</h2>
      </div>
      <span class="readout text-[10px] tracking-[0.16em] text-faint">1/3-OCT · dBFS</span>
    </header>

    <div class="relative rounded-xl border border-hair bg-[#04070a] p-3" style="--mh: 180px">
      <!-- gridlines -->
      <div
        v-for="g in [0, -12, -24, -36, -48, -60]"
        :key="g"
        class="absolute left-3 right-3 border-t border-[rgba(255,255,255,0.05)]"
        :style="{ top: `calc(12px + ${(1 - norm(g)) * 180}px)` }"
      />

      <div class="relative flex items-end gap-[3px]" style="height: var(--mh)">
        <div v-for="i in N" :key="i" class="relative flex-1 h-full flex items-end">
          <div
            class="w-full rounded-t-[2px] rta-bar"
            :style="{ height: `${(disp[i - 1] || 0) * 100}%` }"
          />
          <!-- peak cap -->
          <div
            class="absolute left-0 right-0 h-[2px] bg-white/80"
            :style="{ bottom: `calc(${(peak[i - 1] || 0) * 100}% - 1px)` }"
          />
        </div>
      </div>

      <!-- frequency axis -->
      <div class="flex gap-[3px] mt-1.5">
        <div v-for="i in N" :key="i" class="flex-1 text-center">
          <span v-if="isLabel(i - 1)" class="readout text-[8px] text-faint">{{ fmtHz(ISO_BANDS[i - 1]) }}</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.rta-bar {
  background: linear-gradient(
    to top,
    var(--color-meter-lo) 0%,
    var(--color-meter-lo) 55%,
    var(--color-meter-mid) 82%,
    var(--color-meter-hi) 100%
  );
  background-size: 100% var(--mh);
  background-position: bottom;
  box-shadow: 0 0 8px -2px color-mix(in oklab, var(--color-signal) 50%, transparent);
  transition: height 0.05s linear;
}
</style>
