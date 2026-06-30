<script setup>
import { ref } from 'vue'

const emit = defineEmits(['play-url', 'dlna'])
const url = ref('')

function playUrl() {
  const u = url.value.trim()
  if (!u) return
  emit('play-url', u)
}
</script>

<template>
  <section class="panel rise p-5 md:p-6" style="--accent: var(--color-cool); animation-delay: 120ms">
    <p class="eyebrow mb-0.5">Inputs</p>
    <h2 class="font-display font-bold text-lg tracking-wide text-ink mb-4">SOURCES</h2>

    <!-- cast from yscale-media -->
    <div class="rounded-xl border border-hair bg-[rgba(255,255,255,0.015)] p-4 mb-4">
      <div class="flex items-center gap-2.5">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--color-signal)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M2 16a6 6 0 0 1 6 6M2 12a10 10 0 0 1 10 10M2 20a2 2 0 0 1 2 2" />
          <rect x="2" y="4" width="20" height="14" rx="2" opacity="0.5" />
        </svg>
        <div>
          <p class="font-display font-semibold text-ink text-sm">Cast from yscale-media</p>
          <p class="readout text-[11px] text-dim mt-0.5">Pick a track in the app and tap “Play on mediapi.”</p>
        </div>
      </div>
    </div>

    <!-- DLNA -->
    <button
      class="w-full flex items-center justify-between gap-3 rounded-xl border border-hair bg-[rgba(255,255,255,0.015)] p-4 mb-4 text-left transition-all hover:border-[color-mix(in_oklab,var(--color-violet)_45%,var(--color-edge))]"
      @click="emit('dlna')"
    >
      <div class="flex items-center gap-2.5">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="var(--color-violet)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="14" rx="2" /><path d="M8 21h8M12 18v3" />
        </svg>
        <div>
          <p class="font-display font-semibold text-ink text-sm">DLNA / UPnP renderer</p>
          <p class="readout text-[11px] text-dim mt-0.5">Listen for “mediapi” from any UPnP control app.</p>
        </div>
      </div>
      <span class="chip is-active px-3 py-1.5 text-[12px]" style="--accent: var(--color-violet)">Listen</span>
    </button>

    <!-- URL -->
    <label class="block">
      <span class="eyebrow block mb-1.5">Stream URL · HTTP(S) / HLS / DASH / radio</span>
      <div class="flex gap-2">
        <input
          class="num flex-1"
          type="url"
          inputmode="url"
          autocapitalize="off"
          autocorrect="off"
          spellcheck="false"
          placeholder="https://stream…"
          v-model="url"
          @keydown.enter.prevent="playUrl"
        />
        <button
          class="chip is-active px-4 text-sm flex items-center gap-1.5"
          style="--accent: var(--color-cool)"
          @click="playUrl"
        >
          <svg width="12" height="14" viewBox="0 0 14 16" fill="currentColor"><path d="M0 0l14 8-14 8z" /></svg>
          Play
        </button>
      </div>
    </label>
  </section>
</template>
