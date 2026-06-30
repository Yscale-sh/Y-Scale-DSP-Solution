<script setup>
import { ref } from 'vue'

const props = defineProps({
  channels: { type: Array, default: () => [] }, // cfg.channels (mutated for assignment)
  firs: { type: Array, default: () => [] }, // [{ name, taps }]
})
const emit = defineEmits(['upload', 'delete'])

const fileInput = ref(null)
const busy = ref(false)

async function onFile(e) {
  const file = e.target.files?.[0]
  if (!file) return
  busy.value = true
  try {
    const buffer = await file.arrayBuffer()
    const name = file.name.replace(/\.[^.]+$/, '').slice(0, 64)
    emit('upload', { name, buffer })
  } finally {
    busy.value = false
    e.target.value = ''
  }
}
const tapMs = (taps, fs = 48000) => ((taps / fs) * 1000).toFixed(0)
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-cool); animation-delay: 90ms">
    <header class="flex items-center justify-between gap-3 mb-4 flex-wrap">
      <div>
        <p class="eyebrow">Convolution · Linear-Phase</p>
        <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">FIR · ROOM CORRECTION</h2>
      </div>
      <button
        class="chip is-active px-4 py-2 text-sm flex items-center gap-1.5"
        style="--accent: var(--color-cool)"
        :disabled="busy"
        @click="fileInput?.click()"
      >
        ⤒ {{ busy ? 'Loading…' : 'Load FIR' }}
      </button>
      <input ref="fileInput" type="file" accept=".wav,.txt,.csv,audio/wav" class="hidden" @change="onFile" />
    </header>

    <!-- library -->
    <div v-if="firs.length" class="space-y-2 mb-5">
      <div
        v-for="f in firs"
        :key="f.name"
        class="flex items-center justify-between gap-3 rounded-lg border border-hair bg-[rgba(255,255,255,0.015)] px-3 py-2"
      >
        <div class="min-w-0">
          <span class="font-display font-semibold text-ink text-sm truncate">{{ f.name }}</span>
          <span class="readout text-[10px] text-faint ml-2">{{ f.taps }} taps · {{ tapMs(f.taps) }} ms</span>
        </div>
        <button class="btn-ghost px-2 py-1 text-[10px]" title="Delete FIR" @click="emit('delete', f.name)">✕ Delete</button>
      </div>
    </div>
    <p v-else class="readout text-[12px] text-dim mb-5">
      No filters loaded. Upload a REW-exported impulse-response WAV (or a text list of taps).
    </p>

    <!-- per-channel assignment -->
    <div class="space-y-2.5">
      <p class="eyebrow">Assign per channel</p>
      <div v-for="(ch, i) in channels" :key="i" class="grid grid-cols-[1fr_2fr] gap-3 items-center">
        <span class="readout text-[12px] text-dim truncate">{{ ch.name || 'Channel ' + (i + 1) }}</span>
        <select class="select py-1.5 text-[13px] w-full" v-model="ch.fir" style="--accent: var(--color-cool)">
          <option :value="null">— none (bypass) —</option>
          <option v-for="f in firs" :key="f.name" :value="f.name">{{ f.name }}</option>
        </select>
      </div>
    </div>

    <p class="readout text-[10px] text-faint tracking-[0.1em] mt-4">
      Partitioned FFT convolution per channel. Linear-phase FIRs add ~taps/2 of latency (matched across channels).
      Max 16 384 taps. Export from REW: “Impulse Response → Export as WAV”.
    </p>
  </section>
</template>
