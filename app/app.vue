<script setup lang="ts">
const { catalog, apply: applyTheme } = useTheme()
const { current } = usePreferences()

// El tema depende del catálogo (IPC) y de la preferencia (IPC): se reaplica
// cuando llega cualquiera de los dos. Antes del catálogo, `apply` no hace nada.
watch([catalog, () => current().ui.theme], () => applyTheme(), { immediate: true })

onMounted(() => {
  void useTheme().load()
})
</script>

<template>
  <UApp>
    <NuxtPage />
  </UApp>
</template>
