<script lang="ts">
  import '../app.css';
  import type { LayoutData } from './$types';

  let { children }: { children: any } = $props();
</script>

<svelte:head>
  <title>Voxly</title>
</svelte:head>

<div class="min-h-screen bg-slate-950 text-slate-100 flex flex-col">
  {@render children?.()}
</div>