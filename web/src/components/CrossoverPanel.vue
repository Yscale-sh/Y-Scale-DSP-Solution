<script setup>
import { computed } from 'vue'

const props = defineProps({
  modelValue: { type: Object, default: null }, // { kind, role, freq, order } | null
  accent: { type: String, default: 'var(--color-signal)' },
})
const emit = defineEmits(['update'])

const enabled = computed(() => !!props.modelValue)
const xo = computed(() => props.modelValue || { kind: 'linkwitz_riley', role: 'low_pass', freq: 2000, order: 4 })

function toggle() {
  emit('update', enabled.value ? null : { kind: 'linkwitz_riley', role: 'low_pass', freq: 2000, order: 4 })
}
function patch(key, value) {
  emit('update', { ...xo.value, [key]: value })
}
</script>

<template>
  <div :style="{ '--accent': accent }">
    <div class="flex items-center gap-2.5 mb-3">
      <div class="toggle" :class="{ 'is-on': enabled }" role="switch" :aria-checked="enabled" @click="toggle" />
      <p class="eyebrow">Crossover</p>
      <span v-if="enabled" class="readout text-[10px] text-faint">
        {{ xo.role === 'low_pass' ? 'LPF' : 'HPF' }} · {{ xo.kind === 'linkwitz_riley' ? 'LR' : 'BW' }}{{ xo.order * 6 }}dB/oct
      </span>
    </div>

    <div
      class="grid grid-cols-2 sm:grid-cols-4 gap-2.5 transition-opacity"
      :class="{ 'opacity-30 pointer-events-none': !enabled }"
    >
      <label class="block">
        <span class="eyebrow block mb-1">Type</span>
        <select class="select py-1.5 text-[13px] w-full" :value="xo.kind" :disabled="!enabled" @change="patch('kind', $event.target.value)">
          <option value="linkwitz_riley">Linkwitz-Riley</option>
          <option value="butterworth">Butterworth</option>
        </select>
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Role</span>
        <select class="select py-1.5 text-[13px] w-full" :value="xo.role" :disabled="!enabled" @change="patch('role', $event.target.value)">
          <option value="low_pass">Low Pass</option>
          <option value="high_pass">High Pass</option>
        </select>
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Freq · Hz</span>
        <input
          class="num py-1.5 text-[13px]"
          type="number" min="20" max="20000" step="1"
          :value="Math.round(xo.freq)" :disabled="!enabled"
          @change="patch('freq', +$event.target.value)"
        />
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Order</span>
        <select class="select py-1.5 text-[13px] w-full" :value="xo.order" :disabled="!enabled" @change="patch('order', +$event.target.value)">
          <option :value="1">1 · 6dB</option>
          <option :value="2">2 · 12dB</option>
          <option :value="3">3 · 18dB</option>
          <option :value="4">4 · 24dB</option>
        </select>
      </label>
    </div>
  </div>
</template>
