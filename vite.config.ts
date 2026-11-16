import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve, relative, sep } from 'node:path';

/** 图标数据以内联 SVG 和 JSON 分发，许可文本随构建产物保留在独立目录。 */
function iconDataNotices() {
  const directory = resolve('docs/licenses/icons');
  return {
    name: 'taskrail-icon-data-notices',
    generateBundle(this: import('rollup').PluginContext) {
      for (const entry of readdirSync(directory, { recursive: true, withFileTypes: true })) {
        if (!entry.isFile()) continue;
        const source = resolve(entry.parentPath, entry.name);
        this.emitFile({ type: 'asset', fileName: `icon-licenses/${relative(directory, source).split(sep).join('/')}`, source: readFileSync(source) });
      }
    },
  };
}

export default defineConfig({
  plugins: [svelte(), iconDataNotices()],
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  clearScreen: false,
});
