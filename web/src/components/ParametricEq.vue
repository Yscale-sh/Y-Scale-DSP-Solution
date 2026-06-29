<script setup>
import { BAND_KINDS, bandUsesGain, fmtHz } from '../lib/dsp.js'

const props = defineProps({
  bands: { type: Array, default: () => [] },
  selectedId: { type: [String, Number], default: null },
  accent: { type: String, default: 'var(--color-signal)' },
})
const emit = defineEmits(['add', 'remove', 'update', 'select'])

function patch(id, key, value) {
  emit('update', { id, patch: { [key]: value } })
}
</script>

<template>
  <div>
    <div class="flex items-center justify-between mb-3">
      <p class="eyebrow">Parametric EQ · {{ bands.length }} {{ bands.length === 1 ? 'band' : 'bands' }}</p>
      <button class="btn-ghost px-2.5 py-1.5" @click="emit('add')" :style="{ '--accent': accent }">+ Add band</button>
    </div>

    <p v-if="!bands.length" class="readout text-[12px] text-faint py-3 text-center border border-dashed border-hair rounded-lg">
      Flat response — add a band to start shaping.
    </p>

    <div v-else class="space-y-2">
      <div
        v-for="(b, i) in bands"
        :key="b._id"
        class="grid grid-cols-[auto_1fr] sm:grid-cols-[auto_minmax(0,1.4fr)_repeat(3,minmax(0,1fr))_auto] gap-2 items-center rounded-lg p-2 border transition-colors cursor-pointer"
        :class="
          selectedId === b._id
            ? 'border-[color-mix(in_oklab,var(--accent)_55%,transparent)] bg-[color-mix(in_oklab,var(--accent)_8%,transparent)]'
            : 'border-hair bg-[rgba(255,255,255,0.012)] hover:border-edge'
        "
        :style="{ '--accent': accent }"
        @click="emit('select', b._id)"
      >
        <span
          class="readout text-[11px] font-semibold w-6 h-6 grid place-items-center rounded-md flex-none"
          :style="{ background: 'color-mix(in oklab,' + accent + ' 18%, transparent)', color: accent }"
        >
          {{ i + 1 }}
        </span>

        <select class="select py-1.5 text-[13px]" :value="b.kind" @change="patch(b._id, 'kind', $event.target.value)" @click.stop>
          <option v-for="k in BAND_KINDS" :key="k.id" :value="k.id">{{ k.label }}</option>
        </select>

        <label class="block">
          <span class="eyebrow block mb-0.5 sm:hidden">Freq</span>
          <input
            class="num py-1.5 text-[12px]"
            type="number" min="20" max="20000" step="1"
            :value="Math.round(b.freq)"
            @change="patch(b._id, 'freq', +$event.target.value)" @click.stop
          />
        </label>

        <label class="block">
          <span class="eyebrow block mb-0.5 sm:hidden">Q</span>
          <input
            class="num py-1.5 text-[12px]"
            type="number" min="0.1" max="10" step="0.05"
            :value="b.q"
            @change="patch(b._id, 'q', +$event.target.value)" @click.stop
          />
        </label>

        <label class="block" :class="{ 'opacity-30 pointer-events-none': !bandUsesGain(b.kind) }">
          <span class="eyebrow block mb-0.5 sm:hidden">Gain</span>
          <input
            class="num py-1.5 text-[12px]"
            type="number" min="-24" max="24" step="0.5"
            :value="b.gain_db"
            :disabled="!bandUsesGain(b.kind)"
            @change="patch(b._id, 'gain_db', +$event.target.value)" @click.stop
          />
        </label>

        <button
          class="w-7 h-7 grid place-items-center rounded-md text-faint hover:text-hot hover:bg-[color-mix(in_oklab,var(--color-hot)_14%,transparent)] transition-colors flex-none"
          title="Remove band"
          @click.stop="emit('remove', b._id)"
        >
          <svg width="13" height="13" viewBox="0 0 14 14" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M3 3l8 8M11 3l-8 8" /></svg>
        </button>
      </div>
    </div>
  </div>
</template>
