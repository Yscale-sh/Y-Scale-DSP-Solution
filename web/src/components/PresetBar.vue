<script setup>
import { ref, watch } from 'vue'

const props = defineProps({
  presets: { type: Array, default: () => [] }, // [name]
  active: { type: String, default: null },
})
const emit = defineEmits(['save', 'load', 'delete'])

const sel = ref('')
const newName = ref('')

watch(
  () => props.active,
  (a) => {
    if (a) sel.value = a
  },
  { immediate: true },
)

function load() {
  if (sel.value) emit('load', sel.value)
}
function del() {
  if (sel.value) emit('delete', sel.value)
}
function save() {
  const n = (newName.value || sel.value || '').trim()
  if (n) {
    emit('save', n)
    newName.value = ''
  }
}
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-violet); animation-delay: 0ms">
    <header class="flex items-center justify-between gap-3 mb-4 flex-wrap">
      <div>
        <p class="eyebrow">Tuning Library</p>
        <h2 class="font-display font-bold text-lg tracking-wide text-ink mt-0.5">PRESETS · SCENES</h2>
      </div>
      <div v-if="active" class="flex items-center gap-2 px-3 py-1.5 rounded-full border border-[color-mix(in_oklab,var(--color-violet)_40%,transparent)] bg-[color-mix(in_oklab,var(--color-violet)_10%,transparent)]">
        <span class="w-1.5 h-1.5 rounded-full bg-[var(--color-violet)] dot-live" style="color: var(--color-violet)" />
        <span class="readout text-[11px] tracking-wide text-[var(--color-violet)] truncate max-w-[220px]">{{ active }}</span>
      </div>
      <div v-else class="readout text-[11px] tracking-[0.16em] text-faint uppercase">— unsaved —</div>
    </header>

    <div class="grid sm:grid-cols-[1fr_auto_auto] gap-2.5 items-end mb-3">
      <label class="block">
        <span class="eyebrow block mb-1">Recall a scene</span>
        <select class="select py-2 w-full" v-model="sel" :disabled="presets.length === 0" style="--accent: var(--color-violet)">
          <option v-if="presets.length === 0" value="">No presets saved yet</option>
          <option v-for="p in presets" :key="p" :value="p">{{ p }}</option>
        </select>
      </label>
      <button class="chip is-active px-4 py-2 text-sm" style="--accent: var(--color-violet)" :disabled="!sel" @click="load">
        Load
      </button>
      <button class="chip px-3 py-2 text-sm" style="--accent: var(--color-hot)" :disabled="!sel" title="Delete preset" @click="del">
        Delete
      </button>
    </div>

    <div class="grid sm:grid-cols-[1fr_auto] gap-2.5 items-end">
      <label class="block">
        <span class="eyebrow block mb-1">Save current tuning as</span>
        <input
          class="num py-2"
          type="text"
          maxlength="64"
          :placeholder="active || 'e.g. Living Room · Night'"
          v-model="newName"
          @keydown.enter.prevent="save"
        />
      </label>
      <button class="chip is-active px-4 py-2 text-sm" style="--accent: var(--color-signal)" @click="save">
        ⤓ Save
      </button>
    </div>

    <p class="readout text-[10px] text-faint tracking-[0.1em] mt-3">
      Captures every channel — EQ, crossovers, delays, gains &amp; routing. Recall is instant &amp; click-free.
    </p>
  </section>
</template>
