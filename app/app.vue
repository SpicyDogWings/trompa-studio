<script setup lang="ts">
const tauriVersion = ref<string | null>(null)

onMounted(async () => {
  try {
    const { isTauri } = await import('@tauri-apps/api/core')
    if (!isTauri()) return

    const { getTauriVersion } = await import('@tauri-apps/api/app')
    tauriVersion.value = await getTauriVersion()
  } catch {
    // Fuera de Tauri no hay nada que mostrar.
  }
})

const copiedKey = ref<string | null>(null)
let copyTimer: ReturnType<typeof setTimeout> | undefined

async function copyCommand(key: string, text: string) {
  try {
    await navigator.clipboard.writeText(text)
    copiedKey.value = key
    clearTimeout(copyTimer)
    copyTimer = setTimeout(() => (copiedKey.value = null), 1600)
  } catch {
    copiedKey.value = null
  }
}

onBeforeUnmount(() => clearTimeout(copyTimer))

const heroLinks = [
  {
    label: 'Empezar',
    to: '#empezar',
    trailingIcon: 'i-lucide-arrow-right'
  },
  {
    label: 'Documentación',
    to: 'https://tauri.app/',
    target: '_blank',
    color: 'neutral' as const,
    variant: 'outline' as const,
    icon: 'i-logos-tauri'
  }
]

const features = [
  {
    icon: 'i-logos-nuxt-icon',
    title: 'Nuxt 4',
    description: 'Rutas por archivos, auto-imports y SSR. Genera un frontend estático que Tauri empaqueta sin servidor.'
  },
  {
    icon: 'i-logos-tauri',
    title: 'Tauri 2',
    description: 'Backend en Rust y la webview del sistema. Binarios ligeros, sin arrastrar un navegador completo.'
  },
  {
    icon: 'i-lucide-paintbrush',
    title: 'Nuxt UI 4',
    description: 'Componentes accesibles construidos sobre Tailwind CSS 4, con modo claro y oscuro incluidos.'
  },
  {
    icon: 'i-logos-typescript-icon',
    title: 'TypeScript',
    description: 'Tipado de extremo a extremo: del componente Vue al comando de Rust, sin capas intermedias.'
  },
  {
    icon: 'i-lucide-wifi-off',
    title: 'Iconos offline',
    description: 'Las colecciones de Iconify van empaquetadas en el bundle, así la app funciona sin conexión.'
  },
  {
    icon: 'i-lucide-snowflake',
    title: 'Devbox',
    description: 'Entorno reproducible con Node, Rust y las librerías WebKit/GTK que necesita Tauri en Linux.'
  }
]

const steps = [
  {
    key: 'setup',
    title: 'Prepara el entorno',
    description: 'Descarga las herramientas y las dependencias del proyecto.',
    command: 'devbox install && pnpm install'
  },
  {
    key: 'dev',
    title: 'Levanta la ventana nativa',
    description: 'Nuxt en localhost:3000 y la ventana de Tauri apuntando a él, con recarga en caliente.',
    command: 'pnpm tauri dev'
  },
  {
    key: 'build',
    title: 'Empaqueta la app',
    description: 'Genera el sitio estático y compila el binario con su instalador.',
    command: 'pnpm tauri build'
  }
]

const ctaLinks = [
  {
    label: 'Ver en GitHub',
    to: 'https://github.com/SpicyDogWings/Nuxtri',
    target: '_blank',
    icon: 'i-lucide-github',
    size: 'lg' as const
  },
  {
    label: 'Documentación de Nuxt',
    to: 'https://nuxt.com/docs',
    target: '_blank',
    color: 'neutral' as const,
    variant: 'ghost' as const,
    size: 'lg' as const
  }
]

const navLinks = [
  { label: 'Características', to: '#caracteristicas' },
  { label: 'Empezar', to: '#empezar' },
  { label: 'Stack', to: '#stack' }
]

const footerLinks = [
  { label: 'Nuxt', to: 'https://nuxt.com/docs' },
  { label: 'Tauri', to: 'https://tauri.app/' },
  { label: 'Nuxt UI', to: 'https://ui.nuxt.com/' }
]

useHead({
  title: 'Nuxtri · Plantilla Nuxt + Tauri',
  htmlAttrs: { class: 'scroll-smooth' },
  meta: [
    {
      name: 'description',
      content: 'Plantilla de escritorio con Nuxt 4, Tauri 2, Nuxt UI 4 y Tailwind CSS 4.'
    }
  ]
})
</script>

<template>
  <UApp>
    <div class="min-h-screen bg-default text-default">
      <!-- Header -->
      <UHeader
        title="Nuxtri"
        to="#"
        :ui="{
          title: 'shrink-0 font-bold text-xl text-highlighted flex items-center gap-1.5'
        }"
      >
        <template #title>
          <span class="flex size-9 items-center justify-center rounded-xl bg-gradient-to-br from-blue-500 to-violet-600 shadow-sm">
            <UIcon name="i-logos-nuxt-icon" class="size-5 text-white" />
          </span>
          <span>Nuxtri</span>
          <UBadge color="neutral" variant="subtle" size="sm" class="hidden sm:inline-flex">
            v0.1
          </UBadge>
        </template>

        <UNavigationMenu :items="navLinks" color="neutral" />

        <template #right>
          <UButton
            to="https://github.com/SpicyDogWings/Nuxtri"
            target="_blank"
            color="neutral"
            variant="ghost"
            icon="i-lucide-github"
            aria-label="Repositorio en GitHub"
          />
          <UColorModeButton color="neutral" variant="ghost" />
        </template>

        <template #body>
          <UNavigationMenu :items="navLinks" orientation="vertical" />
        </template>
      </UHeader>

      <UMain>
        <!-- Hero -->
        <UPageHero
          orientation="horizontal"
          :ui="{
            container: 'py-16 sm:py-20 lg:py-24 gap-12 sm:gap-y-16',
            title: 'text-4xl sm:text-5xl lg:text-6xl'
          }"
        >
          <template #headline>
            <div class="flex flex-wrap items-center gap-2">
              <UBadge color="primary" variant="subtle" icon="i-logos-nuxt-icon">Nuxt 4</UBadge>
              <UBadge color="secondary" variant="subtle" icon="i-logos-tauri">Tauri 2</UBadge>
              <UBadge color="info" variant="subtle" icon="i-lucide-paintbrush">Nuxt UI 4</UBadge>
            </div>
          </template>

          <template #title>
            Apps de escritorio con la
            <span class="bg-gradient-to-r from-blue-600 via-violet-600 to-fuchsia-600 bg-clip-text text-transparent dark:from-blue-400 dark:via-violet-400 dark:to-fuchsia-400">
              velocidad de Nuxt
            </span>
          </template>

          <template #description>
            Frontend con Vue y Tailwind, backend en Rust, y un binario final que pesa poco.
            Todo el cariño de Nuxt dentro de una ventana nativa.
          </template>

          <template #footer>
            <div class="flex flex-wrap gap-x-4 gap-y-3">
              <UButton
                v-for="link in heroLinks"
                :key="link.label"
                size="xl"
                v-bind="link"
              />
            </div>

            <div class="mt-6 flex flex-wrap items-center gap-2 text-sm">
              <UBadge v-if="tauriVersion" color="success" variant="subtle" icon="i-lucide-monitor">
                Ejecutándose en Tauri v{{ tauriVersion }}
              </UBadge>
              <UBadge v-else color="neutral" variant="subtle" icon="i-lucide-globe">
                Modo navegador
              </UBadge>
              <span class="text-muted">
                {{ tauriVersion ? 'Ventana nativa detectada' : 'Usa pnpm tauri dev para la ventana nativa' }}
              </span>
            </div>
          </template>

          <!-- Mock de terminal -->
          <div class="relative">
            <div class="absolute -inset-1 rounded-3xl bg-gradient-to-br from-blue-500/30 to-violet-500/30 blur-2xl" />
            <div class="relative overflow-hidden rounded-2xl border border-neutral-800 bg-neutral-950 shadow-2xl">
              <div class="flex items-center gap-2 border-b border-white/10 px-4 py-3">
                <span class="size-3 rounded-full bg-red-500/90" />
                <span class="size-3 rounded-full bg-yellow-500/90" />
                <span class="size-3 rounded-full bg-green-500/90" />
                <span class="ml-2 font-mono text-xs text-white/40">nuxtri — zsh</span>
              </div>
              <div class="space-y-1.5 p-5 font-mono text-xs leading-relaxed text-white/70 sm:text-sm">
                <p class="whitespace-nowrap">
                  <span class="text-emerald-400">➜ </span><span class="text-sky-400">nuxtri </span><span class="text-white/90">pnpm tauri dev</span>
                </p>
                <p class="text-white/40">Nuxt 4.6.2 · Nitro 2.13 · Vite 8</p>
                <p class="text-white/40">
                  <span class="text-emerald-400">➜ </span>Local:
                  <span class="text-white/80">http://localhost:3000/</span>
                </p>
                <p class="text-white/40">Compiling app v0.1.0</p>
                <p class="text-white/40">
                  <span class="text-emerald-400">Finished</span> dev profile in 1.4s
                </p>
                <p class="text-white/40">Running <span class="text-white/80">target/debug/app</span></p>
                <p class="text-white/20">▍</p>
              </div>
            </div>
          </div>
        </UPageHero>

        <!-- Características -->
        <UPageSection
          id="caracteristicas"
          headline="Stack incluido"
          title="Todo lo que necesitas, ya conectado"
          description="Sin configuración manual entre capas: Nuxt genera el frontend, Tauri lo empaqueta y Nuxt UI se encarga de que se vea bien."
          class="scroll-mt-20 border-t border-default"
          :ui="{ container: 'py-16 sm:py-20 lg:py-24 gap-8 sm:gap-12' }"
        >
          <UPageGrid>
            <UPageCard
              v-for="feature in features"
              :key="feature.title"
              :icon="feature.icon"
              :title="feature.title"
              :description="feature.description"
              variant="subtle"
              spotlight
            />
          </UPageGrid>
        </UPageSection>

        <!-- Empezar -->
        <UPageSection
          id="empezar"
          headline="En tres pasos"
          title="Levanta tu app"
          description="Con el entorno de Devbox no necesitas instalar Node, Rust ni las librerías de WebKit a mano."
          class="scroll-mt-20 border-t border-default bg-elevated/40"
          :ui="{ container: 'py-16 sm:py-20 lg:py-24 gap-8 sm:gap-12' }"
        >
          <UPageGrid>
            <UPageCard
              v-for="(step, index) in steps"
              :key="step.key"
              :title="step.title"
              :description="step.description"
              variant="subtle"
            >
              <template #leading>
                <span class="flex size-8 items-center justify-center rounded-full bg-primary text-sm font-semibold text-inverted">
                  {{ index + 1 }}
                </span>
              </template>

              <template #footer>
                <div class="flex items-center justify-between gap-2 rounded-lg bg-default px-3 py-2 ring ring-default">
                  <code class="truncate font-mono text-xs text-toned">{{ step.command }}</code>
                  <UButton
                    :icon="copiedKey === step.key ? 'i-lucide-check' : 'i-lucide-copy'"
                    :color="copiedKey === step.key ? 'success' : 'neutral'"
                    variant="ghost"
                    size="xs"
                    :aria-label="copiedKey === step.key ? 'Comando copiado' : 'Copiar comando'"
                    @click="copyCommand(step.key, step.command)"
                  />
                </div>
              </template>
            </UPageCard>
          </UPageGrid>

          <p class="mt-8 text-center text-sm text-muted">
            En Zed, las mismas tareas están en
            <code class="rounded bg-elevated px-1.5 py-0.5 font-mono text-xs">.zed/tasks.json</code>
            → <span class="font-medium text-highlighted">task: spawn</span>
          </p>
        </UPageSection>

        <!-- CTA + stack -->
        <UContainer id="stack" class="scroll-mt-20 py-16 sm:py-24">
          <UPageCTA
            title="¿Listo para construir?"
            description="Clona el repo, ejecuta pnpm tauri dev y empieza a añadir tus propias pantallas."
            :links="ctaLinks"
            variant="subtle"
          >
            <template #body>
              <div class="flex flex-wrap justify-center gap-3">
                <UBadge variant="subtle" size="lg" color="success" icon="i-logos-nuxt-icon">Nuxt 4</UBadge>
                <UBadge variant="subtle" size="lg" color="warning" icon="i-logos-tauri">Tauri 2</UBadge>
                <UBadge variant="subtle" size="lg" color="info" icon="i-logos-vue">Vue 3</UBadge>
                <UBadge variant="subtle" size="lg" color="primary" icon="i-logos-tailwindcss-icon">Tailwind 4</UBadge>
                <UBadge variant="subtle" size="lg" color="error" icon="i-logos-rust">Rust</UBadge>
              </div>
            </template>
          </UPageCTA>
        </UContainer>
      </UMain>

      <!-- Footer -->
      <UFooter class="border-t border-default">
        <template #left>
          <p class="text-sm text-muted">Nuxtri · Plantilla Nuxt + Tauri · MIT</p>
        </template>

        <template #right>
          <UButton
            v-for="link in footerLinks"
            :key="link.label"
            v-bind="link"
            target="_blank"
            color="neutral"
            variant="ghost"
            size="sm"
          />
          <span v-if="tauriVersion" class="text-sm text-dimmed">Tauri v{{ tauriVersion }}</span>
        </template>
      </UFooter>
    </div>
  </UApp>
</template>
