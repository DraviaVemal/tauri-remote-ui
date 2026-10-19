import typescript from '@rollup/plugin-typescript';
import { execSync } from 'child_process';
import fg from 'fast-glob';
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'fs';
import { basename, dirname, join } from 'path';
import { defineConfig, type Plugin } from 'rollup';
import { fileURLToPath } from 'url';

const __dirname = fileURLToPath(new URL('.', import.meta.url))
const distDir = join(__dirname, './dist');
rmSync(distDir, { recursive: true, force: true });
mkdirSync(distDir, { recursive: true });

// Read the original package.json
const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));
const modules = fg.sync(['!./src/*.d.ts', './src/*.ts'])
const version = resolveVersion();

export default defineConfig([
  {
    input: Object.fromEntries(modules.map((p) => [basename(p, '.ts'), p])),
    output: [
      {
        format: 'esm',
        dir: './dist',
        preserveModules: true,
        preserveModulesRoot: 'src',
        entryFileNames: (chunkInfo) => {
          if (chunkInfo.name.includes('node_modules')) {
            return externalLibPath(chunkInfo.name) + '.js'
          }

          return '[name].js'
        }
      },
      {
        format: 'cjs',
        dir: './dist',
        preserveModules: true,
        preserveModulesRoot: 'src',
        entryFileNames: (chunkInfo) => {
          if (chunkInfo.name.includes('node_modules')) {
            return externalLibPath(chunkInfo.name) + '.cjs'
          }

          return '[name].cjs'
        }
      }
    ],
    plugins: [
      injectVersion(),
      typescript({
        declaration: true,
        declarationDir: './dist',
        rootDir: 'src'
      }),
      preparePackageFile()
    ],
    external: [
      /^@tauri-apps\/api/,
      ...Object.keys(pkg.dependencies || {}),
      ...Object.keys(pkg.peerDependencies || {})
    ]
  }
])

/** APP_VERSION, else latest git tag when DEVOPS_BUILD=1, else package.json. */
function resolveVersion(): string {
  if (process.env.APP_VERSION) {
    return process.env.APP_VERSION;
  }
  if (process.env.DEVOPS_BUILD === "1") {
    execSync('git fetch --tags --force', { stdio: 'inherit' });
    let versionTag: string;
    try {
      versionTag = execSync('git describe --tags --match v* --abbrev=0').toString().trim();
    } catch (err) {
      throw new Error('Error retrieving Git tag');
    }
    const tagVersion = versionTag.replace(/^v/, '');
    console.log(`Resolved version ${tagVersion} from git tag`);
    return tagVersion;
  }
  return pkg.version;
}

/** Replaces the contents of src/version.ts with the resolved version. */
function injectVersion(): Plugin {
  return {
    name: 'inject-version',
    transform(_code, id) {
      if (id.replace(/\\/g, '/').endsWith('/src/version.ts')) {
        return { code: `export const PACKAGE_VERSION = ${JSON.stringify(version)};`, map: null };
      }
      return null;
    },
    // Declarations are emitted from the on-disk placeholder, so rewrite them.
    writeBundle() {
      writeFileSync(
        join(distDir, 'version.d.ts'),
        `export declare const PACKAGE_VERSION = ${JSON.stringify(version)};\n`
      );
    }
  };
}

function preparePackageFile() {
  // Select only the properties you want to include
  const publishPkg = {
    name: pkg.name,
    license: pkg.license,
    version,
    author: pkg.author,
    description: pkg.description,
    type: pkg.type,
    keywords: pkg.keywords,
    homepage: pkg.homepage,
    types: './index.d.ts',
    main: './index.cjs',
    module: './index.js',
    repository: pkg.repository,
    publishConfig: pkg.publishConfig,
    exports: {
      ".": {
        "types": "./index.d.ts",
        "import": "./index.js",
        "require": "./index.cjs"
      },
      "./api/app": {
        "types": "./api/app/index.d.ts",
        "import": "./api/app/index.js",
        "require": "./api/app/index.cjs"
      },
      "./api/core": {
        "types": "./api/core/index.d.ts",
        "import": "./api/core/index.js",
        "require": "./api/core/index.cjs"
      },
      "./api/event": {
        "types": "./api/event/index.d.ts",
        "import": "./api/event/index.js",
        "require": "./api/event/index.cjs"
      }
    },
    typesVersions: {
      '*': {
        'api/app/*': [
          './api/app/*.d.ts'
        ],
        'api/core/*': [
          './api/core/*.d.ts'
        ],
        'api/event/*': [
          './api/event/*.d.ts'
        ]
      }
    },
    // Include peer dependencies
    peerDependencies: pkg.peerDependencies
  };

  // Write the modified package.json to the dist directory
  writeFileSync(
    join('./dist', 'package.json'),
    JSON.stringify(publishPkg, null, 2)
  );

  // Copy README
  copyFileSync('../README.md', join('./dist', 'README.md'));
  // Copy LICENSE
  copyFileSync('../LICENSE', join('./dist', 'LICENSE'));
}

function externalLibPath(path: string) {
  return `external/${basename(dirname(path))}/${basename(path)}`
}