<script setup>
import { computed } from 'vue'

const props = defineProps({
  modelValue: { type: Object, default: null }, // { kind, role, freq, order, freq_high? } | null
  accent: { type: String, default: 'var(--color-signal)' },
})
const emit = defineEmits(['update'])

const DEFAULT = { kind: 'linkwitz_riley', role: 'low_pass', freq: 2000, order: 4, freq_high: null }

const enabled = computed(() => !!props.modelValue)
const xo = computed(() => props.modelValue || DEFAULT)
const isBand = computed(() => xo.value.role === 'band_pass')

// Valid orders per alignment: Bessel 1–4, Linkwitz-Riley even 2–8, Butterworth 1–8.
const orderOpts = computed(() => {
  if (xo.value.kind === 'bessel') return [1, 2, 3, 4]
  if (xo.value.kind === 'linkwitz_riley') return [2, 4, 6, 8]
  return [1, 2, 3, 4, 5, 6, 7, 8]
})

const KIND_LABEL = { butterworth: 'BW', linkwitz_riley: 'LR', bessel: 'BESL' }
const ROLE_LABEL = { low_pass: 'LPF', high_pass: 'HPF', band_pass: 'BPF' }

function nearestOrder(kind, order) {
  if (kind === 'bessel') return Math.min(4, Math.max(1, order))
  if (kind === 'linkwitz_riley') return Math.min(8, Math.max(2, Math.round(order / 2) * 2))
  return Math.min(8, Math.max(1, order))
}

function toggle() {
  emit('update', enabled.value ? null : { ...DEFAULT })
}
function patch(key, value) {
  const next = { ...xo.value, [key]: value }
  if (key === 'kind') next.order = nearestOrder(value, next.order)
  if (key === 'role' && value === 'band_pass' && next.freq_high == null) {
    next.freq_high = Math.max(Math.round(next.freq * 4), next.freq + 200)
  }
  emit('update', next)
}
</script>

<template>
  <div :style="{ '--accent': accent }">
    <div class="flex items-center gap-2.5 mb-3 flex-wrap">
      <div class="toggle" :class="{ 'is-on': enabled }" role="switch" :aria-checked="enabled" @click="toggle" />
      <p class="eyebrow">Crossover</p>
      <span v-if="enabled" class="readout text-[10px] text-faint">
        {{ ROLE_LABEL[xo.role] }} · {{ KIND_LABEL[xo.kind] }}{{ xo.order * 6 }}dB/oct
        <template v-if="isBand"> · {{ Math.round(xo.freq) }}–{{ Math.round(xo.freq_high || 0) }} Hz</template>
      </span>
    </div>

    <div
      class="grid gap-2.5 transition-opacity"
      :class="[isBand ? 'grid-cols-2 sm:grid-cols-5' : 'grid-cols-2 sm:grid-cols-4', { 'opacity-30 pointer-events-none': !enabled }]"
    >
      <label class="block">
        <span class="eyebrow block mb-1">Type</span>
        <select class="select py-1.5 text-[13px] w-full" :value="xo.kind" :disabled="!enabled" @change="patch('kind', $event.target.value)">
          <option value="linkwitz_riley">Linkwitz-Riley</option>
          <option value="butterworth">Butterworth</option>
          <option value="bessel">Bessel</option>
        </select>
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Role</span>
        <select class="select py-1.5 text-[13px] w-full" :value="xo.role" :disabled="!enabled" @change="patch('role', $event.target.value)">
          <option value="low_pass">Low Pass</option>
          <option value="high_pass">High Pass</option>
          <option value="band_pass">Band Pass</option>
        </select>
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">{{ isBand ? 'Low · Hz' : 'Freq · Hz' }}</span>
        <input
          class="num py-1.5 text-[13px]"
          type="number" min="20" max="20000" step="1"
          :value="Math.round(xo.freq)" :disabled="!enabled"
          @change="patch('freq', +$event.target.value)"
        />
      </label>
      <label v-if="isBand" class="block">
        <span class="eyebrow block mb-1">High · Hz</span>
        <input
          class="num py-1.5 text-[13px]"
          type="number" min="20" max="20000" step="1"
          :value="Math.round(xo.freq_high || 0)" :disabled="!enabled"
          @change="patch('freq_high', +$event.target.value)"
        />
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Order</span>
        <select class="select py-1.5 text-[13px] w-full" :value="xo.order" :disabled="!enabled" @change="patch('order', +$event.target.value)">
          <option v-for="o in orderOpts" :key="o" :value="o">{{ o }} · {{ o * 6 }}dB</option>
        </select>
      </label>
    </div>
  </div>
</template>
