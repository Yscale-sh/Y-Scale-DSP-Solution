<script setup>
import { ref, reactive, computed } from 'vue'
import { lin2db, clamp, fmtDb } from '../lib/dsp.js'

const emit = defineEmits(['play', 'stop'])
defineProps({
  nowPlaying: { type: String, default: '' },
})

const SOURCES = [
  { id: 'sine', label: 'Sine' },
  { id: 'sweep', label: 'Sweep' },
  { id: 'pink', label: 'Pink' },
  { id: 'white', label: 'White' },
  { id: 'impulse', label: 'Impulse' },
  { id: 'file', label: 'File' },
]

const kind = ref('sine')
const amp = ref(0.03) // SAFE low default ≈ −30 dBFS
const p = reactive({
  freq: 1000,
  f1: 20,
  f2: 20000,
  dur: 10,
  sweepLoop: true,
  periodMs: 500,
  path: '',
  fileLoop: true,
})

const ampDb = computed(() => lin2db(amp.value))
const isHot = computed(() => amp.value > 0.2)
const usesAmp = computed(() => kind.value !== 'file')
const ampFill = computed(() => `${Math.sqrt(clamp(amp.value, 0, 1)) * 100}%`)

function setAmpFromSlider(e) {
  // perceptual (square) mapping so the low/safe end has fine resolution
  const x = clamp(parseFloat(e.target.value), 0, 1)
  amp.value = +(x * x).toFixed(4)
}
const ampSlider = computed(() => Math.sqrt(clamp(amp.value, 0, 1)))

function buildSpec() {
  switch (kind.value) {
    case 'sine':
      return { kind: 'sine', freq: +p.freq, amp: +amp.value }
    case 'sweep':
      return { kind: 'sweep', f1: +p.f1, f2: +p.f2, dur: +p.dur, amp: +amp.value, looping: !!p.sweepLoop }
    case 'pink':
      return { kind: 'pink', amp: +amp.value }
    case 'white':
      return { kind: 'white', amp: +amp.value }
    case 'impulse':
      return { kind: 'impulse', period_ms: +p.periodMs, amp: +amp.value }
    case 'file':
      return { kind: 'file', path: p.path, looping: !!p.fileLoop }
    default:
      return { kind: 'silence' }
  }
}

const label = computed(() => SOURCES.find((s) => s.id === kind.value)?.label || kind.value)

function play() {
  if (kind.value === 'file' && !p.path.trim()) return
  emit('play', { spec: buildSpec(), label: label.value })
}
function stop() {
  emit('stop')
}
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-signal); animation-delay: 0ms">
    <header class="flex items-center justify-between gap-3 mb-4 flex-wrap">
      <div>
        <p class="eyebrow">Signal Source · Transport</p>
        <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">GENERATOR</h2>
      </div>
      <div
        v-if="nowPlaying"
        class="flex items-center gap-2 px-3 py-1.5 rounded-full border border-[color-mix(in_oklab,var(--color-signal)_40%,transparent)] bg-[color-mix(in_oklab,var(--color-signal)_10%,transparent)]"
      >
        <span class="w-1.5 h-1.5 rounded-full bg-signal dot-live" style="color: var(--color-signal)" />
        <span class="readout text-[11px] tracking-wide text-signal">{{ nowPlaying }}</span>
      </div>
      <div v-else class="readout text-[11px] tracking-[0.16em] text-faint uppercase">— idle —</div>
    </header>

    <!-- source selector -->
    <div class="flex flex-wrap gap-2 mb-5">
      <button
        v-for="s in SOURCES"
        :key="s.id"
        class="chip px-3.5 py-2 text-sm"
        :class="{ 'is-active': kind === s.id }"
        @click="kind = s.id"
      >
        {{ s.label }}
      </button>
    </div>

    <div class="grid lg:grid-cols-[1fr_auto] gap-6 items-end">
      <!-- params + amp -->
      <div class="space-y-5">
        <!-- per-kind params -->
        <div class="min-h-[58px]">
          <div v-if="kind === 'sine'" class="grid grid-cols-2 sm:grid-cols-3 gap-3 max-w-md">
            <label class="block">
              <span class="eyebrow block mb-1">Freq · Hz</span>
              <input class="num" type="number" min="20" max="20000" step="1" v-model.number="p.freq" />
            </label>
          </div>

          <div v-else-if="kind === 'sweep'" class="grid grid-cols-2 sm:grid-cols-4 gap-3">
            <label class="block">
              <span class="eyebrow block mb-1">From · Hz</span>
              <input class="num" type="number" min="20" max="20000" v-model.number="p.f1" />
            </label>
            <label class="block">
              <span class="eyebrow block mb-1">To · Hz</span>
              <input class="num" type="number" min="20" max="20000" v-model.number="p.f2" />
            </label>
            <label class="block">
              <span class="eyebrow block mb-1">Dur · s</span>
              <input class="num" type="number" min="0.1" step="0.1" v-model.number="p.dur" />
            </label>
            <button
              class="chip self-end h-[35px] text-[12px]"
              :class="{ 'is-active': p.sweepLoop }"
              @click="p.sweepLoop = !p.sweepLoop"
            >
              ⟳ Loop
            </button>
          </div>

          <div v-else-if="kind === 'impulse'" class="grid grid-cols-2 gap-3 max-w-xs">
            <label class="block">
              <span class="eyebrow block mb-1">Period · ms</span>
              <input class="num" type="number" min="1" step="1" v-model.number="p.periodMs" />
            </label>
          </div>

          <div v-else-if="kind === 'file'" class="grid grid-cols-[1fr_auto] gap-3 items-end max-w-xl">
            <label class="block">
              <span class="eyebrow block mb-1">WAV path on device</span>
              <input class="num" type="text" placeholder="/home/pi/test.wav" v-model="p.path" />
            </label>
            <button
              class="chip h-[35px] text-[12px] px-3"
              :class="{ 'is-active': p.fileLoop }"
              @click="p.fileLoop = !p.fileLoop"
            >
              ⟳ Loop
            </button>
          </div>

          <p v-else class="readout text-[12px] text-dim pt-2">
            Broadband {{ kind === 'pink' ? 'pink' : 'white' }} noise — full-spectrum excitation.
          </p>
        </div>

        <!-- amplitude (prominent + safe) -->
        <div
          v-if="usesAmp"
          class="rounded-xl border p-4"
          :class="
            isHot
              ? 'border-[color-mix(in_oklab,var(--color-hot)_55%,transparent)] bg-[color-mix(in_oklab,var(--color-hot)_8%,transparent)]'
              : 'border-hair bg-[rgba(255,255,255,0.015)]'
          "
        >
          <div class="flex items-baseline justify-between mb-1">
            <span class="eyebrow">Output Level · Amplitude</span>
            <span
              v-if="isHot"
              class="readout text-[10px] font-semibold tracking-[0.2em] text-hot animate-pulse"
              >⚠ HOT OUTPUT</span
            >
          </div>
          <div class="flex items-center gap-4">
            <input
              type="range"
              class="fader flex-1"
              :class="{ 'is-hot': isHot }"
              min="0"
              max="1"
              step="0.001"
              :value="ampSlider"
              :style="{ '--fill': ampFill, '--accent': isHot ? 'var(--color-hot)' : 'var(--color-signal)' }"
              @input="setAmpFromSlider"
            />
            <div class="text-right tabular-nums leading-none">
              <div
                class="readout text-2xl font-semibold"
                :style="{ color: isHot ? 'var(--color-hot)' : 'var(--color-signal)' }"
              >
                {{ fmtDb(ampDb, 1) }}
              </div>
              <div class="readout text-[10px] text-faint tracking-[0.12em]">dBFS · {{ amp.toFixed(3) }}</div>
            </div>
          </div>
        </div>
      </div>

      <!-- transport actions -->
      <div class="flex lg:flex-col gap-3 lg:w-44">
        <button
          class="group relative flex-1 lg:flex-none flex items-center justify-center gap-2 py-4 px-5 rounded-xl font-display font-bold tracking-wide text-base text-void overflow-hidden"
          style="
            background: linear-gradient(180deg, color-mix(in oklab, var(--color-signal) 100%, white 10%), var(--color-signal-deep));
            box-shadow: 0 14px 40px -14px color-mix(in oklab, var(--color-signal) 80%, transparent);
          "
          @click="play"
        >
          <svg width="14" height="16" viewBox="0 0 14 16" fill="currentColor"><path d="M0 0l14 8-14 8z" /></svg>
          PLAY
        </button>
        <button
          class="flex-1 lg:flex-none flex items-center justify-center gap-2 py-4 px-5 rounded-xl font-display font-bold tracking-wide text-base text-ink border-2 transition-all"
          style="
            border-color: color-mix(in oklab, var(--color-hot) 60%, transparent);
            background: color-mix(in oklab, var(--color-hot) 12%, transparent);
          "
          @click="stop"
        >
          <span class="w-3 h-3 rounded-[2px] bg-hot" style="box-shadow: 0 0 10px var(--color-hot)" />
          STOP
        </button>
      </div>
    </div>
  </section>
</template>
