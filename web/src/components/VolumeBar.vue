<script setup>
import { computed } from 'vue'

const props = defineProps({
  volume: { type: Object, required: true }, // { pct, db, muted }
})
const emit = defineEmits(['set', 'mute']) // set(pct), mute(bool)

const pct = computed(() => Math.round(props.volume?.pct ?? 0))
const db = computed(() => props.volume?.db ?? -60)
const muted = computed(() => !!props.volume?.muted)

function onInput(e) {
  emit('set', parseFloat(e.target.value))
}
function toggleMute() {
  emit('mute', !muted.value)
}

// icon waves shown by level
const waves = computed(() => (muted.value ? 0 : pct.value > 66 ? 3 : pct.value > 33 ? 2 : pct.value > 2 ? 1 : 0))
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-signal); animation-delay: 60ms">
    <div class="flex items-center justify-between mb-4">
      <div>
        <p class="eyebrow">Master · Output Level</p>
        <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">VOLUME</h2>
      </div>
      <div class="text-right leading-none">
        <div v-if="muted" class="readout text-3xl font-semibold text-hot tracking-tight">MUTE</div>
        <div v-else class="readout text-3xl font-semibold tabular-nums text-signal">
          {{ pct }}<span class="text-base text-faint">%</span>
        </div>
        <div class="readout text-[10px] tracking-[0.12em] mt-1" :class="muted ? 'text-hot' : 'text-faint'">
          {{ muted ? 'silenced' : db.toFixed(1) + ' dB · DAC' }}
        </div>
      </div>
    </div>

    <div class="flex items-center gap-4">
      <!-- mute / speaker -->
      <button class="mute" :class="{ 'is-muted': muted }" title="Mute" @click="toggleMute">
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 9v6h4l5 4V5L8 9H4z" fill="currentColor" stroke="none" />
          <template v-if="!muted">
            <path v-if="waves >= 1" d="M16 9.5a3.5 3.5 0 0 1 0 5" />
            <path v-if="waves >= 2" d="M18.5 7a7 7 0 0 1 0 10" />
            <path v-if="waves >= 3" d="M21 4.5a10.5 10.5 0 0 1 0 15" />
          </template>
          <template v-else>
            <path d="M22 9l-6 6M16 9l6 6" stroke="var(--color-hot)" />
          </template>
        </svg>
      </button>

      <input
        type="range"
        class="fader flex-1"
        min="0"
        max="100"
        step="1"
        :value="muted ? 0 : pct"
        :style="{ '--fill': (muted ? 0 : pct) + '%' }"
        @input="onInput"
      />
    </div>

    <p class="readout text-[10px] text-faint tracking-[0.1em] mt-3">
      Drives the DAC's hardware digital volume — applies to every source, saved across reboots.
    </p>
  </section>
</template>

<style scoped>
.mute {
  display: grid;
  place-items: center;
  width: 46px;
  height: 46px;
  border-radius: 12px;
  flex: none;
  color: var(--color-dim);
  border: 1px solid var(--color-hair);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), rgba(255, 255, 255, 0));
  transition: all 0.16s ease;
}
.mute:hover {
  color: var(--color-ink);
  border-color: color-mix(in oklab, var(--color-signal) 40%, var(--color-edge));
}
.mute.is-muted {
  color: var(--color-hot);
  border-color: color-mix(in oklab, var(--color-hot) 45%, transparent);
  background: color-mix(in oklab, var(--color-hot) 10%, transparent);
}
</style>
