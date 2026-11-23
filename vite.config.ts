import { defineConfig, lazyPlugins } from "vite-plus";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  lint: {
    plugins: ["typescript", "react"],
    jsPlugins: [
      "./tools/oxlint/conductor.ts",
      {
        name: "vite-plus",
        specifier: "vite-plus/oxlint-plugin",
      },
    ],
    categories: {
      correctness: "off",
    },
    env: {
      builtin: true,
    },
    ignorePatterns: [
      "**/node_modules/**",
      "dist/**",
      "dist-ssr/**",
      "src-tauri/target/**",
      "src-tauri/gen/**",
      ".agent-app/**",
      "src/bindings.ts",
    ],
    rules: {
      "constructor-super": "error",
      "for-direction": "error",
      "getter-return": "error",
      "no-async-promise-executor": "error",
      "no-case-declarations": "error",
      "no-class-assign": "error",
      "no-compare-neg-zero": "error",
      "no-cond-assign": "error",
      "no-const-assign": "error",
      "no-constant-binary-expression": "error",
      "no-constant-condition": "error",
      "no-control-regex": "error",
      "no-debugger": "error",
      "no-delete-var": "error",
      "no-dupe-class-members": "error",
      "no-dupe-else-if": "error",
      "no-dupe-keys": "error",
      "no-duplicate-case": "error",
      "no-empty": "error",
      "no-empty-character-class": "error",
      "no-empty-pattern": "error",
      "no-empty-static-block": "error",
      "no-ex-assign": "error",
      "no-extra-boolean-cast": "error",
      "no-fallthrough": "error",
      "no-func-assign": "error",
      "no-global-assign": "error",
      "no-import-assign": "error",
      "no-invalid-regexp": "error",
      "no-irregular-whitespace": "error",
      "no-loss-of-precision": "error",
      "no-misleading-character-class": "error",
      "no-new-native-nonconstructor": "error",
      "no-nonoctal-decimal-escape": "error",
      "no-obj-calls": "error",
      "no-prototype-builtins": "error",
      "no-redeclare": "error",
      "no-regex-spaces": "error",
      "no-self-assign": "error",
      "no-setter-return": "error",
      "no-shadow-restricted-names": "error",
      "no-sparse-arrays": "error",
      "no-this-before-super": "error",
      "no-unassigned-vars": "error",
      "no-unexpected-multiline": "error",
      "no-unreachable": "error",
      "no-unsafe-finally": "error",
      "no-unsafe-negation": "error",
      "no-unsafe-optional-chaining": "error",
      "no-unused-labels": "error",
      "no-unused-private-class-members": "error",
      "no-unused-vars": "error",
      "no-useless-backreference": "error",
      "no-useless-catch": "error",
      "no-useless-escape": "error",
      "no-with": "error",
      "preserve-caught-error": "error",
      "require-yield": "error",
      "use-isnan": "error",
      "valid-typeof": "error",
      "no-array-constructor": "error",
      "no-unused-expressions": "error",
      "typescript/ban-ts-comment": "error",
      "typescript/no-duplicate-enum-values": "error",
      "typescript/no-empty-object-type": "error",
      "typescript/no-explicit-any": "error",
      "typescript/no-extra-non-null-assertion": "error",
      "typescript/no-misused-new": "error",
      "typescript/no-namespace": "error",
      "typescript/no-non-null-asserted-optional-chain": "error",
      "typescript/no-require-imports": "error",
      "typescript/no-this-alias": "error",
      "typescript/no-unnecessary-type-constraint": "error",
      "typescript/no-unsafe-declaration-merging": "error",
      "typescript/no-unsafe-function-type": "error",
      "typescript/no-wrapper-object-types": "error",
      "typescript/prefer-as-const": "error",
      "typescript/prefer-namespace-keyword": "error",
      "typescript/triple-slash-reference": "error",
      "no-useless-assignment": "error",
      "react/rules-of-hooks": "error",
      "react/exhaustive-deps": "error",
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "@tauri-apps/plugin-updater",
              allowTypeImports: true,
              message:
                "Not needed at startup. Import it only in AboutPane.tsx, which loads with Settings.",
            },
            {
              name: "@tauri-apps/plugin-process",
              allowTypeImports: true,
              message:
                "Not needed at startup. Import it only in AboutPane.tsx, which loads with Settings.",
            },
          ],
          patterns: [
            {
              group: ["@codemirror/*", "@lezer/*"],
              allowTypeImports: true,
              message:
                "The editor stays out of the main chunk. Import it only in ResponseViewer.tsx and RawBodyEditor.tsx, which load lazily.",
            },
          ],
        },
      ],
      "vite-plus/prefer-vite-plus-imports": "error",
    },
    overrides: [
      {
        files: ["**/*.{ts,tsx,mts,cts}"],
        rules: {
          "constructor-super": "off",
          "getter-return": "off",
          "no-class-assign": "off",
          "no-const-assign": "off",
          "no-dupe-class-members": "off",
          "no-dupe-keys": "off",
          "no-func-assign": "off",
          "no-import-assign": "off",
          "no-new-native-nonconstructor": "off",
          "no-obj-calls": "off",
          "no-redeclare": "off",
          "no-setter-return": "off",
          "no-this-before-super": "off",
          "no-unreachable": "off",
          "no-unsafe-negation": "off",
          "no-with": "off",
          "no-var": "error",
          "prefer-const": "error",
          "prefer-rest-params": "error",
          "prefer-spread": "error",
        },
      },
      {
        files: [
          "src/features/requests/ResponseViewer.tsx",
          "src/features/requests/RawBodyEditor.tsx",
        ],
        rules: {
          "no-restricted-imports": [
            "error",
            {
              paths: [
                {
                  name: "@tauri-apps/plugin-updater",
                  allowTypeImports: true,
                  message:
                    "Not needed at startup. Import it only in AboutPane.tsx, which loads with Settings.",
                },
                {
                  name: "@tauri-apps/plugin-process",
                  allowTypeImports: true,
                  message:
                    "Not needed at startup. Import it only in AboutPane.tsx, which loads with Settings.",
                },
              ],
            },
          ],
        },
      },
      {
        files: ["src/features/settings/AboutPane.tsx"],
        rules: {
          "no-restricted-imports": [
            "error",
            {
              patterns: [
                {
                  group: ["@codemirror/*", "@lezer/*"],
                  allowTypeImports: true,
                  message:
                    "The editor stays out of the main chunk. Import it only in ResponseViewer.tsx and RawBodyEditor.tsx, which load lazily.",
                },
              ],
            },
          ],
        },
      },
      {
        files: ["**/*.{js,jsx,mjs,cjs}"],
        rules: {
          "no-undef": "error",
        },
      },
      {
        files: ["src/**/*.{js,jsx,ts,tsx,mjs,cjs,mts,cts}"],
        env: {
          browser: true,
        },
      },
      {
        files: [
          "*.{js,ts,mjs,cjs,mts,cts}",
          "scripts/**/*.{js,ts,mjs,cjs,mts,cts}",
        ],
        env: {
          node: true,
        },
      },
    ],
  },
  fmt: {
    printWidth: 80,
    sortPackageJson: false,
    ignorePatterns: [
      "node_modules/",
      "dist/",
      "dist-ssr/",
      "src-tauri/",
      ".agent-app/",
      "src/bindings.ts",
      "pnpm-lock.yaml",
      "postman-collection-example/",
      "*.postman_collection.json",
      "*.postman_environment.json",
      "extensions.json",
    ],
  },
  plugins: lazyPlugins(() => [react(), tailwindcss()]),
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
});
