<script setup>
import { computed } from 'vue'

const props = defineProps({
  bass: { type: Object, required: true }, // { enabled, freq, order, rumble_hz }
})

const enabled = computed(() => !!props.bass.enabled)
const rumbleOn = computed(() => props.bass.rumble_hz > 1)
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-violet); animation-delay: 60ms">
    <header class="flex items-center justify-between gap-3 mb-4 flex-wrap">
      <div class="flex items-center gap-2.5">
        <div
          class="toggle"
          :class="{ 'is-on': enabled }"
          role="switch"
          :aria-checked="enabled"
          @click="bass.enabled = !bass.enabled"
        />
        <div>
          <p class="eyebrow">Low-End</p>
          <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">BASS MANAGEMENT</h2>
        </div>
      </div>
      <span v-if="enabled" class="readout text-[11px] text-[var(--color-violet)]">
        Mono &lt; {{ Math.round(bass.freq) }} Hz · LR{{ bass.order * 6 }}<template v-if="rumbleOn"> · rumble {{ Math.round(bass.rumble_hz) }} Hz</template>
      </span>
    </header>

    <div
      class="grid grid-cols-2 sm:grid-cols-3 gap-3 transition-opacity"
      :class="{ 'opacity-30 pointer-events-none': !enabled }"
    >
      <label class="block">
        <span class="eyebrow block mb-1">Crossover · Hz</span>
        <input class="num py-1.5 text-[13px]" type="number" min="20" max="500" step="1" v-model.number="bass.freq" :disabled="!enabled" />
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Slope</span>
        <select class="select py-1.5 text-[13px] w-full" v-model.number="bass.order" :disabled="!enabled" style="--accent: var(--color-violet)">
          <option :value="2">LR12 dB/oct</option>
          <option :value="4">LR24 dB/oct</option>
          <option :value="6">LR36 dB/oct</option>
          <option :value="8">LR48 dB/oct</option>
        </select>
      </label>
      <label class="block">
        <span class="eyebrow block mb-1">Rumble HPF · Hz</span>
        <input class="num py-1.5 text-[13px]" type="number" min="0" max="60" step="1" v-model.number="bass.rumble_hz" :disabled="!enabled" placeholder="0 = off" />
      </label>
    </div>

    <p class="readout text-[10px] text-faint tracking-[0.1em] mt-3">
      Sums everything below the crossover to mono (tighter, room-mode-friendly bass) and high-passes the mains.
      The optional rumble filter blocks sub-sonic energy. Ready to route to a dedicated sub on multi-channel hardware.
    </p>
  </section>
</template>
