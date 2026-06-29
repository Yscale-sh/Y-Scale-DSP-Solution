<script setup>
import { ref, computed } from 'vue'
import { uid } from '../lib/util.js'
import { clamp, fmtDb } from '../lib/dsp.js'
import EqCurve from './EqCurve.vue'
import ParametricEq from './ParametricEq.vue'
import GraphicEq from './GraphicEq.vue'
import CrossoverPanel from './CrossoverPanel.vue'

const props = defineProps({
  channel: { type: Object, required: true },
  accent: { type: String, default: 'var(--color-signal)' },
  fs: { type: Number, default: 48000 },
  index: { type: Number, default: 0 },
})

const selectedId = ref(null)

const gainFill = computed(() => `${((clamp(props.channel.gain_db, -60, 12) + 60) / 72) * 100}%`)
const delayTotalMs = computed(() => props.channel.delay_ms + props.channel.delay_cm * 0.029155)

// ── band mutation (single owner) ────────────────────────────────────────────
function addBand() {
  const b = { _id: uid('band'), kind: 'peaking', freq: 1000, q: 1.0, gain_db: 0 }
  props.channel.eq.push(b)
  selectedId.value = b._id
}
function removeBand(id) {
  const i = props.channel.eq.findIndex((b) => b._id === id)
  if (i >= 0) props.channel.eq.splice(i, 1)
  if (selectedId.value === id) selectedId.value = null
}
function updateBand({ id, patch }) {
  const b = props.channel.eq.find((x) => x._id === id)
  if (b) Object.assign(b, patch)
}

function setGraphic(arr) {
  props.channel.graphic_eq = arr
}
function setCrossover(obj) {
  props.channel.crossover = obj
}

function resetChannel() {
  const c = props.channel
  c.gain_db = 0
  c.delay_ms = 0
  c.delay_cm = 0
  c.invert = false
  c.mute = false
  c.eq.splice(0, c.eq.length)
  c.graphic_eq = null
  c.crossover = null
  selectedId.value = null
}
</script>

<template>
  <section
    class="panel rise overflow-hidden transition-opacity"
    :class="{ 'opacity-60': channel.mute }"
    :style="{ '--accent': accent, animationDelay: 240 + index * 90 + 'ms' }"
  >
    <!-- accent header bar -->
    <div class="h-1" :style="{ background: `linear-gradient(90deg, ${accent}, transparent)` }" />

    <div class="p-5 md:p-6 space-y-6">
      <!-- header -->
      <header class="flex items-center gap-3">
        <span
          class="w-2.5 h-2.5 rounded-full flex-none"
          :style="{ background: accent, boxShadow: `0 0 12px ${accent}` }"
        />
        <input
          class="flex-1 min-w-0 bg-transparent font-display font-bold text-xl tracking-wide text-ink outline-none focus:text-white"
          :style="{ caretColor: accent }"
          v-model="channel.name"
          spellcheck="false"
          aria-label="Channel name"
        />
        <button
          class="btn-ghost px-2 py-1 text-[10px]"
          title="Reset channel"
          @click="resetChannel"
        >
          ⟲ Reset
        </button>
        <button
          class="chip px-3 py-1.5 text-xs"
          :class="{ 'is-active': channel.invert }"
          title="Polarity invert"
          @click="channel.invert = !channel.invert"
        >
          ø INV
        </button>
        <button
          class="chip px-3 py-1.5 text-xs"
          :class="{ 'is-active': channel.mute }"
          :style="channel.mute ? '--accent: var(--color-hot)' : ''"
          @click="channel.mute = !channel.mute"
        >
          MUTE
        </button>
      </header>

      <!-- gain + delay -->
      <div class="grid sm:grid-cols-2 gap-5">
        <div>
          <div class="flex items-baseline justify-between mb-1.5">
            <span class="eyebrow">Gain</span>
            <span class="readout text-sm font-semibold" :style="{ color: accent }">{{ fmtDb(channel.gain_db, 1) }} dB</span>
          </div>
          <input
            type="range" class="fader" min="-60" max="12" step="0.5"
            v-model.number="channel.gain_db"
            :style="{ '--fill': gainFill }"
          />
        </div>

        <div>
          <div class="flex items-baseline justify-between mb-1.5">
            <span class="eyebrow">Delay · Time Align</span>
            <span class="readout text-[11px] text-faint">≈ {{ delayTotalMs.toFixed(2) }} ms total</span>
          </div>
          <div class="grid grid-cols-2 gap-2.5">
            <label class="block">
              <span class="readout text-[9px] text-faint block mb-0.5">milliseconds</span>
              <input class="num py-1.5 text-[13px]" type="number" min="0" step="0.01" v-model.number="channel.delay_ms" />
            </label>
            <label class="block">
              <span class="readout text-[9px] text-faint block mb-0.5">centimetres</span>
              <input class="num py-1.5 text-[13px]" type="number" min="0" step="0.1" v-model.number="channel.delay_cm" />
            </label>
          </div>
        </div>
      </div>

      <!-- response curve (signature) -->
      <div class="rounded-xl border border-hair bg-[#04070a] p-3">
        <div class="flex items-center justify-between mb-1 px-1">
          <span class="eyebrow">Magnitude Response · ±24 dB</span>
          <span class="readout text-[10px] text-faint">drag nodes — x: freq · y: gain</span>
        </div>
        <EqCurve
          :bands="channel.eq"
          :crossover="channel.crossover"
          :graphic-eq="channel.graphic_eq"
          :fs="fs"
          :accent="accent"
          :selected-id="selectedId"
          :height="220"
          @band-input="updateBand"
          @select="selectedId = $event"
        />
      </div>

      <!-- parametric EQ -->
      <ParametricEq
        :bands="channel.eq"
        :selected-id="selectedId"
        :accent="accent"
        @add="addBand"
        @remove="removeBand"
        @update="updateBand"
        @select="selectedId = $event"
      />

      <div class="border-t border-hair" />

      <!-- crossover -->
      <CrossoverPanel :model-value="channel.crossover" :accent="accent" @update="setCrossover" />

      <div class="border-t border-hair" />

      <!-- graphic EQ -->
      <GraphicEq :model-value="channel.graphic_eq" :accent="accent" @update="setGraphic" />
    </div>
  </section>
</template>
