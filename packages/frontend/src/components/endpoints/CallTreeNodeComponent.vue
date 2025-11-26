<script setup lang="ts">
import type { CallTreeNode, CallType } from '@gitlab-org/gkg';
import { computed } from 'vue';

import { ChevronDown, ChevronRight } from 'lucide-vue-next';

import { Badge } from '@/components/ui/badge';

interface Props {
  node: CallTreeNode;
  depth: number;
  isExpanded: boolean;
}

const props = defineProps<Props>();

const emit = defineEmits<{
  toggle: [nodeFqn: string];
}>();

const hasChildren = computed(() => props.node.children.length > 0);

const getCallTypeIcon = (callType: CallType, isExternal: boolean) => {
  if (isExternal) return '🔴';
  switch (callType) {
    case 'Direct':
      return '🔵';
    case 'Ambiguous':
      return '🟠';
    case 'Interface':
      return '🟡';
    case 'Abstract':
      return '🟡';
    default:
      return '🔵';
  }
};

const getCallTypeBadgeClass = (callType: CallType, isExternal: boolean) => {
  if (isExternal) {
    return 'bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30';
  }
  switch (callType) {
    case 'Direct':
      return 'bg-blue-500/20 text-blue-700 dark:text-blue-400 border-blue-500/30';
    case 'Ambiguous':
      return 'bg-orange-500/20 text-orange-700 dark:text-orange-400 border-orange-500/30';
    case 'Interface':
      return 'bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30';
    case 'Abstract':
      return 'bg-yellow-500/20 text-yellow-700 dark:text-yellow-400 border-yellow-500/30';
    default:
      return 'bg-blue-500/20 text-blue-700 dark:text-blue-400 border-blue-500/30';
  }
};

const getFileName = (filePath: string) => {
  const parts = filePath.split('/');
  return parts[parts.length - 1] || filePath;
};

const handleToggle = () => {
  emit('toggle', props.node.method_fqn);
};
</script>

<template>
  <div>
    <div
      class="flex items-center gap-2 py-1 px-2 hover:bg-muted/50 rounded cursor-pointer whitespace-nowrap"
      :style="{ paddingLeft: depth * 24 + 8 + 'px' }"
      @click="handleToggle"
    >
      <ChevronRight
        v-if="hasChildren && !isExpanded"
        class="h-4 w-4 text-muted-foreground flex-shrink-0"
      />
      <ChevronDown
        v-else-if="hasChildren && isExpanded"
        class="h-4 w-4 text-muted-foreground flex-shrink-0"
      />
      <div v-else class="w-4" />

      <span class="text-lg">{{ getCallTypeIcon(node.call_type, node.is_external) }}</span>

      <code class="text-sm font-mono flex-1">
        <span v-if="node.class_name" class="text-muted-foreground">{{ node.class_name }}.</span
        >{{ node.method_name }}()
      </code>

      <Badge
        v-if="node.is_external"
        variant="outline"
        class="text-xs bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30"
      >
        External
      </Badge>
      <Badge
        v-else
        variant="outline"
        :class="['text-xs', getCallTypeBadgeClass(node.call_type, node.is_external)]"
      >
        {{ node.call_type }}
      </Badge>

      <span class="text-xs text-muted-foreground">
        {{ getFileName(node.file_path) }}:{{ node.start_line }}
      </span>
    </div>

    <div v-if="hasChildren && isExpanded">
      <CallTreeNodeComponent
        v-for="child in node.children"
        :key="child.method_fqn"
        :node="child"
        :depth="depth + 1"
        :is-expanded="isExpanded"
        @toggle="emit('toggle', $event)"
      />
    </div>
  </div>
</template>
