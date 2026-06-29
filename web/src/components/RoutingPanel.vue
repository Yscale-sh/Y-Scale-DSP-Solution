<script setup>
const props = defineProps({
  modelValue: { type: String, default: 'stereo' },
})
const emit = defineEmits(['update:modelValue'])

const PRESETS = [
  { id: 'stereo', label: 'Stereo', sub: 'L→L  R→R' },
  { id: 'mono', label: 'Mono', sub: '(L+R)→both' },
  { id: 'left_to_both', label: 'Left → Both', sub: 'L→L  L→R' },
  { id: 'right_to_both', label: 'Right → Both', sub: 'R→L  R→R' },
  { id: 'swap', label: 'Swap', sub: 'L↔R' },
]
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-cool); animation-delay: 160ms">
    <header class="mb-4">
      <p class="eyebrow">Input → Output</p>
      <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">ROUTING MATRIX</h2>
    </header>
    <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-2.5">
      <button
        v-for="r in PRESETS"
        :key="r.id"
        class="chip flex flex-col items-start gap-0.5 px-3.5 py-3 text-left"
        :class="{ 'is-active': modelValue === r.id }"
        @click="emit('update:modelValue', r.id)"
      >
        <span class="font-display font-semibold text-sm">{{ r.label }}</span>
        <span class="readout text-[10px] opacity-70 tracking-wide">{{ r.sub }}</span>
      </button>
    </div>
  </section>
</template>
