<script setup lang="ts">
import { watch } from 'vue'
import { useConnection } from '~/composables/useConnection'
import { useSchema } from '~/composables/useSchema'

useHead({ title: 'Conectar · Trompa Studio' })

const sslOptions = [
  { label: 'Preferir (por defecto)', value: 'prefer' },
  { label: 'Requerir', value: 'require' },
  { label: 'Desactivar', value: 'disable' }
]

const {
  isDesktop,
  isConnecting,
  showPassword,
  mode,
  fields,
  url,
  error,
  connection,
  connectionLabel,
  restore,
  checkDesktop,
  connect,
  disconnect
} = useConnection()

const {
  tree,
  selected,
  isLoading: schemaLoading,
  error: schemaError,
  position,
  select,
  setPosition,
  load: loadSchema,
  reset: resetSchema,
  restorePosition
} = useSchema()

watch(
  connection,
  (value) => {
    if (value) void loadSchema()
    else resetSchema()
  },
  { immediate: true }
)

onMounted(async () => {
  restore()
  restorePosition()
  await checkDesktop()
})
</script>

<template>
  <div
    v-if="!connection"
    class="relative flex min-h-screen flex-col items-center justify-center overflow-hidden bg-default px-4 py-10"
  >
    <div class="pointer-events-none absolute inset-0" aria-hidden="true">
      <div class="absolute -top-40 left-1/2 size-[40rem] -translate-x-1/2 rounded-full bg-primary/10 blur-3xl" />
      <div class="absolute -bottom-48 right-0 size-[30rem] rounded-full bg-violet-500/10 blur-3xl" />
    </div>

    <div class="relative w-full max-w-xl">
      <div class="mb-8 text-center">
        <div class="mx-auto mb-4 flex size-14 items-center justify-center rounded-2xl bg-gradient-to-br from-emerald-500 to-teal-600 shadow-lg shadow-emerald-500/20">
          <UIcon name="i-lucide-database" class="size-7 text-white" />
        </div>
        <h1 class="text-2xl font-bold tracking-tight text-highlighted">Trompa Studio</h1>
        <p class="mt-1 text-sm text-muted">
          Conecta a tu base de datos PostgreSQL
        </p>
      </div>

      <UCard :ui="{ body: 'p-6 sm:p-8' }">
        <div class="mb-6 grid grid-cols-2 gap-1 rounded-xl bg-elevated p-1">
          <UButton
            label="Campos"
            icon="i-lucide-text-cursor-input"
            color="neutral"
            :variant="mode === 'fields' ? 'soft' : 'ghost'"
            block
            @click="mode = 'fields'"
          />
          <UButton
            label="URL"
            icon="i-lucide-link"
            color="neutral"
            :variant="mode === 'url' ? 'soft' : 'ghost'"
            block
            @click="mode = 'url'"
          />
        </div>

        <form class="space-y-4" @submit.prevent="connect">
          <template v-if="mode === 'fields'">
            <div class="grid gap-4 sm:grid-cols-3">
              <UFormField label="Host" class="sm:col-span-2">
                <UInput
                  v-model="fields.host"
                  icon="i-lucide-server"
                  placeholder="localhost"
                  class="w-full"
                  autocomplete="off"
                  spellcheck="false"
                />
              </UFormField>
              <UFormField label="Puerto">
                <UInput v-model="fields.port" inputmode="numeric" placeholder="5432" class="w-full" />
              </UFormField>
            </div>

            <div class="grid gap-4 sm:grid-cols-2">
              <UFormField label="Base de datos">
                <UInput
                  v-model="fields.database"
                  icon="i-lucide-database"
                  placeholder="postgres"
                  class="w-full"
                  autocomplete="off"
                  spellcheck="false"
                />
              </UFormField>
              <UFormField label="Usuario">
                <UInput
                  v-model="fields.user"
                  icon="i-lucide-user"
                  placeholder="postgres"
                  class="w-full"
                  autocomplete="off"
                  spellcheck="false"
                />
              </UFormField>
            </div>

            <div class="grid gap-4 sm:grid-cols-2">
              <UFormField label="Contraseña" hint="Opcional">
                <UInput
                  v-model="fields.password"
                  :type="showPassword ? 'text' : 'password'"
                  icon="i-lucide-key-round"
                  placeholder="••••••••"
                  class="w-full"
                  autocomplete="current-password"
                >
                  <template #trailing>
                    <UButton
                      type="button"
                      :icon="showPassword ? 'i-lucide-eye-off' : 'i-lucide-eye'"
                      color="neutral"
                      variant="link"
                      size="xs"
                      :aria-label="showPassword ? 'Ocultar contraseña' : 'Mostrar contraseña'"
                      @click="showPassword = !showPassword"
                    />
                  </template>
                </UInput>
              </UFormField>

              <UFormField label="SSL">
                <USelect v-model="fields.sslmode" :items="sslOptions" class="w-full" />
              </UFormField>
            </div>
          </template>

          <UFormField
            v-else
            label="URL de conexión"
            hint="postgresql://usuario:contraseña@host:5432/base"
          >
            <UInput
              v-model="url"
              icon="i-lucide-link"
              placeholder="postgresql://postgres@localhost:5432/postgres"
              class="w-full font-mono text-sm"
              autocomplete="off"
              spellcheck="false"
            />
          </UFormField>

          <UFormField
            label="Certificado CA (PEM)"
            hint="Opcional, para servidores con CA propia"
          >
            <UInput
              v-model="fields.sslrootcert"
              icon="i-lucide-shield-check"
              placeholder="/ruta/al/ca.pem"
              class="w-full font-mono text-sm"
              autocomplete="off"
              spellcheck="false"
            />
          </UFormField>

          <UAlert
            v-if="!isDesktop"
            color="warning"
            variant="subtle"
            icon="i-lucide-monitor-off"
            title="Modo navegador"
            description="La conexión se ejecuta en el backend Rust. Abre la app con pnpm tauri dev para conectar."
          />

          <UAlert
            v-if="error"
            color="error"
            variant="subtle"
            icon="i-lucide-circle-alert"
            :title="error.message"
            :description="error.code ? `SQLSTATE ${error.code}` : undefined"
          />

          <UButton
            type="submit"
            label="Conectar"
            icon="i-lucide-plug-zap"
            size="lg"
            block
            :loading="isConnecting"
            :disabled="!isDesktop || isConnecting"
          />
        </form>
      </UCard>

      <p class="mt-6 text-center text-xs text-dimmed">
        Se recuerdan host, puerto, base, usuario, SSL y la ruta de la CA. La contraseña nunca se guarda.
      </p>
    </div>
  </div>

  <div v-else class="flex h-screen overflow-hidden bg-default">
    <main class="min-w-0 flex-1 overflow-y-auto p-6 sm:p-8">
      <div class="mx-auto max-w-3xl">
        <UCard :ui="{ body: 'p-6 sm:p-8' }">
          <div class="flex items-start gap-4">
            <div class="flex size-11 shrink-0 items-center justify-center rounded-xl bg-success/10">
              <UIcon name="i-lucide-database-zap" class="size-6 text-success" />
            </div>
            <div class="min-w-0">
              <h2 class="text-lg font-semibold text-highlighted">Conectado</h2>
              <p class="truncate font-mono text-sm text-muted" :title="connectionLabel">
                {{ connectionLabel }}
              </p>
            </div>
          </div>

          <dl class="mt-6 grid grid-cols-1 gap-4 rounded-xl bg-elevated/60 p-4 text-sm sm:grid-cols-3">
            <div class="min-w-0">
              <dt class="text-muted">Base</dt>
              <dd class="mt-0.5 truncate font-medium text-highlighted">{{ connection.database }}</dd>
            </div>
            <div class="min-w-0">
              <dt class="text-muted">Usuario</dt>
              <dd class="mt-0.5 truncate font-medium text-highlighted">{{ connection.user }}</dd>
            </div>
            <div class="min-w-0">
              <dt class="text-muted">Servidor</dt>
              <dd class="mt-0.5 truncate font-medium text-highlighted" :title="connection.serverVersion">
                {{ connection.serverVersion }}
              </dd>
            </div>
          </dl>

          <UButton
            label="Desconectar"
            color="neutral"
            variant="outline"
            icon="i-lucide-unplug"
            block
            class="mt-6"
            @click="disconnect"
          />
        </UCard>
      </div>
    </main>

    <SchemaSidebar
      :class="position === 'left' ? 'order-first border-r' : 'order-last border-l'"
      :database="connection.database"
      :items="tree"
      :selected="selected"
      :loading="schemaLoading"
      :error="schemaError"
      :position="position"
      @update:selected="select"
      @update:position="setPosition"
      @retry="loadSchema"
    />
  </div>
</template>
