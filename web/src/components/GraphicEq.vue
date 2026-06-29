<script setup>
import { ref, computed } from 'vue'
import { ISO_BANDS, fmtHz, clamp } from '../lib/dsp.js'

const props = defineProps({
  modelValue: { type: Array, default: null }, // 30 gains or null
  accent: { type: String, default: 'var(--color-signal)' },
})
const emit = defineEmits(['update'])

const enabled = computed(() => Array.isArray(props.modelValue))
const gains = computed(() => props.modelValue || new Array(30).fill(0))
const RANGE = 12
const tracks = ref([])
let dragIdx = -1

function emitGains(arr) {
  emit('update', arr.map((v) => +clamp(v, -RANGE, RANGE).toFixed(2)))
}

function toggle() {
  if (enabled.value) emit('update', null)
  else emitGains(new Array(30).fill(0))
}
function flat() {
  emitGains(new Array(30).fill(0))
}

function valueFromEvent(i, clientY) {
  const el = tracks.value[i]
  if (!el) return gains.value[i]
  const r = el.getBoundingClientRect()
  const half = r.height / 2
  const frac = (r.top + half - clientY) / half
  return clamp(Math.round(frac * RANGE * 2) / 2, -RANGE, RANGE)
}
function setIdx(i, val) {
  const arr = gains.value.slice()
  arr[i] = val
  emitGains(arr)
}
function onDown(e, i) {
  if (!enabled.value) return
  dragIdx = i
  e.currentTarget.setPointerCapture?.(e.pointerId)
  setIdx(i, valueFromEvent(i, e.clientY))
}
function onMove(e, i) {
  if (dragIdx !== i) return
  setIdx(i, valueFromEvent(i, e.clientY))
}
function onUp() {
  dragIdx = -1
}
function onKey(e, i) {
  if (!enabled.value) return
  const step = e.shiftKey ? 2 : 0.5
  if (e.key === 'ArrowUp') {
    setIdx(i, clamp(gains.value[i] + step, -RANGE, RANGE))
    e.preventDefault()
  } else if (e.key === 'ArrowDown') {
    setIdx(i, clamp(gains.value[i] - step, -RANGE, RANGE))
    e.preventDefault()
  } else if (e.key === 'Home' || e.key === '0') {
    setIdx(i, 0)
    e.preventDefault()
  }
}

const pct = (v) => (Math.abs(v) / RANGE) * 50
</script>

<template>
  <div :style="{ '--accent': accent }">
    <div class="flex items-center justify-between mb-3 gap-3 flex-wrap">
      <div class="flex items-center gap-2.5">
        <div class="toggle" :class="{ 'is-on': enabled }" role="switch" :aria-checked="enabled" @click="toggle" />
        <p class="eyebrow">30-Band Graphic EQ</p>
      </div>
      <button class="btn-ghost px-2.5 py-1.5" :disabled="!enabled" :class="{ 'opacity-30': !enabled }" @click="flat">
        ⟲ Flat
      </button>
    </div>

    <div
      class="relative rounded-xl border border-hair bg-[#04070a] p-3 overflow-x-auto"
      :class="{ 'opacity-40 pointer-events-none': !enabled }"
    >
      <div class="flex gap-[3px] sm:gap-1.5 min-w-[560px]">
        <div v-for="(g, i) in gains" :key="i" class="flex-1 flex flex-col items-center gap-1">
          <span
            class="readout text-[8px] tabular-nums h-3 leading-3 transition-opacity"
            :style="{ opacity: Math.abs(g) > 0.05 ? 1 : 0, color: g >= 0 ? accent : 'var(--color-hot)' }"
          >
            {{ g > 0 ? '+' + g : g }}
          </span>
          <div
            :ref="(el) => (tracks[i] = el)"
            class="relative w-full max-w-[16px] h-[120px] rounded-full bg-[rgba(255,255,255,0.04)] cursor-ns-resize touch-none"
            role="slider"
            tabindex="0"
            :aria-label="fmtHz(ISO_BANDS[i]) + ' Hz'"
            :aria-valuenow="g"
            aria-valuemin="-12"
            aria-valuemax="12"
            @pointerdown="onDown($event, i)"
            @pointermove="onMove($event, i)"
            @pointerup="onUp"
            @pointercancel="onUp"
            @keydown="onKey($event, i)"
          >
            <!-- center line -->
            <div class="absolute left-0 right-0 top-1/2 h-px bg-[rgba(255,255,255,0.12)]" />
            <!-- fill -->
            <div
              class="absolute left-0 right-0 rounded-full"
              :style="
                g >= 0
                  ? { bottom: '50%', height: pct(g) + '%', background: accent, opacity: 0.85 }
                  : { top: '50%', height: pct(g) + '%', background: 'var(--color-hot)', opacity: 0.8 }
              "
            />
            <!-- thumb -->
            <div
              class="absolute left-1/2 -translate-x-1/2 w-[20px] h-[7px] rounded-[3px] -translate-y-1/2"
              :style="{
                top: `calc(50% - ${(g / RANGE) * 50}%)`,
                background: g >= 0 ? '#eafffb' : '#ffe3ea',
                boxShadow: `0 0 8px ${g >= 0 ? accent : 'var(--color-hot)'}`,
              }"
            />
          </div>
          <span class="readout text-[7px] sm:text-[8px] text-faint rotate-0 whitespace-nowrap leading-none h-3">
            {{ fmtHz(ISO_BANDS[i]) }}
          </span>
        </div>
      </div>
    </div>
  </div>
</template>
