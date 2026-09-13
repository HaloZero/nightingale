import path from 'node:path';

// Mirrors .oxlintrc.json's ignorePatterns. oxlint already excludes these
// itself, but it exits non-zero when every file it's explicitly handed is
// ignored ("No files found to lint"), so a commit touching only ignored
// files (generated bindings, this config file itself, ...) would otherwise
// fail the hook. Filtering here avoids that without changing what oxlint
// actually lints.
const isIgnoredByOxlint = (file) =>
  file.includes('/dist/') ||
  file.includes('/node_modules/') ||
  file.includes('/target/') ||
  file.includes('/src/types/') ||
  file.includes('/src-tauri/gen/') ||
  path.basename(file).includes('.config.');

export default {
  '*.{js,jsx,ts,tsx,mjs,cjs}': (files) => {
    const lintable = files.filter((file) => !isIgnoredByOxlint(file));
    return lintable.length > 0 ? `oxlint ${lintable.join(' ')}` : [];
  },
  '*.css': 'pnpm exec stylelint',
  '*': 'pnpm run format:staged',
};
