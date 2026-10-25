<script setup lang="ts">
import { computed } from 'vue'
import type { TableTab } from '~/composables/useTableTabs'
import { pageRange, tableColumns } from '~/utils/table'

const props = defineProps<{ tab: TableTab }>()

const emit = defineEmits<{ 'update:page': [page: number]; retry: [] }>()

const range = computed(() => pageRange(props.tab.page, props.tab.pageSize, props.tab.total))
</script>

<template>
  <p v-if="tab.kind !== 'table' && tab.kind !== 'view'" class="text-sm text-muted">
    Las funciones y enums no tienen registros que listar.
  </p>

  <UAlert
    v-else-if="tab.error"
    color="error"
    variant="subtle"
    icon="i-lucide-circle-alert"
    :title="tab.error.message"
    :description="tab.error.code ? `SQLSTATE ${tab.error.code}` : undefined"
  >
    <template #actions>
      <UButton
        label="Reintentar"
        color="error"
        variant="soft"
        size="xs"
        icon="i-lucide-rotate-cw"
        @click="emit('retry')"
      />
    </template>
  </UAlert>

  <div v-else class="flex h-full flex-col gap-3">
    <div class="min-h-0 flex-1 overflow-auto rounded-lg border border-default">
      <UTable
        :data="tab.rows"
        :columns="tableColumns(tab.columns)"
        :loading="tab.status === 'loading'"
        empty="No hay registros en esta página."
        :ui="{
          th: 'border-e border-b border-accented bg-elevated/60 last:border-e-0',
          td: 'border-e border-b border-accented last:border-e-0'
        }"
      >
        <template #loading>
          <UIcon name="i-lucide-loader-circle" class="size-5 animate-spin" />
        </template>
      </UTable>
    </div>

    <div v-if="tab.total > 0" class="flex shrink-0 items-center justify-between gap-3">
      <p class="text-xs text-muted">{{ range.from }}–{{ range.to }} de {{ tab.total }}</p>
      <UPagination
        :total="tab.total"
        :items-per-page="tab.pageSize"
        :page="tab.page"
        @update:page="emit('update:page', $event)"
      />
    </div>
  </div>
</template>
