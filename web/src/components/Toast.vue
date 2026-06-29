<script setup>
defineProps({
  toast: { type: Object, default: null }, // { kind: 'ok'|'error', msg: string }
})
</script>

<template>
  <Transition name="toast">
    <div
      v-if="toast"
      class="fixed left-1/2 -translate-x-1/2 z-[100] flex items-center gap-3 px-4 py-2.5 rounded-xl border backdrop-blur-md"
      :class="
        toast.kind === 'error'
          ? 'border-[color-mix(in_oklab,var(--color-hot)_55%,transparent)] bg-[color-mix(in_oklab,var(--color-hot)_16%,#0a0d10)]'
          : 'border-[color-mix(in_oklab,var(--color-signal)_45%,transparent)] bg-[color-mix(in_oklab,var(--color-signal)_12%,#0a0d10)]'
      "
      style="bottom: max(20px, env(safe-area-inset-bottom))"
      role="status"
      aria-live="polite"
    >
      <span
        class="w-2 h-2 rounded-full flex-none"
        :class="toast.kind === 'error' ? 'bg-hot' : 'bg-signal'"
        :style="
          toast.kind === 'error'
            ? 'box-shadow:0 0 10px var(--color-hot)'
            : 'box-shadow:0 0 10px var(--color-signal)'
        "
      />
      <span class="readout text-[12px] tracking-wide text-ink">{{ toast.msg }}</span>
    </div>
  </Transition>
</template>

<style scoped>
.toast-enter-active {
  animation: toast-in 0.32s cubic-bezier(0.22, 1, 0.36, 1);
}
.toast-leave-active {
  transition: opacity 0.25s ease, transform 0.25s ease;
}
.toast-leave-to {
  opacity: 0;
  transform: translateY(10px);
}
</style>
