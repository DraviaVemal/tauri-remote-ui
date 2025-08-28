import { defineConfig } from 'rollup'
import typescript from '@rollup/plugin-typescript'
import fg from 'fast-glob'
import { basename, dirname, join } from 'path'
import { opendirSync, rmSync, Dir, readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'fs'
import { fileURLToPath } from 'url'

// cleanup dist dir
const __dirname = fileURLToPath(new URL('.', import.meta.url))
cleanDir(join(__dirname, './dist'))
mkdirSync(join(__dirname, './dist'), { recursive: true });

// Read the original package.json
const pkg = JSON.parse(readFileSync('./package.json', 'utf8'));
const modules = fg.sync(['!./src/*.d.ts', './src/*.ts'])

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

function preparePackageFile() {

  // Select only the properties you want to include
  const publishPkg = {
    name: pkg.name,
    license: pkg.license,
    version: pkg.version,
    author: pkg.author,
    description: pkg.description,
    type: pkg.type,
    types: pkg.types,
    main: pkg.main,
    module: pkg.module,
    repository: pkg.repository,
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

function cleanDir(path: string) {
  let dir: Dir
  try {
    dir = opendirSync(path)
  } catch (err: any) {
    switch (err.code) {
      case 'ENOENT':
        return // Noop when directory don't exists.
      case 'ENOTDIR':
        throw new Error(`'${path}' is not a directory.`)
      default:
        throw err
    }
  }

  let file = dir.readSync()
  while (file) {
    const filePath = join(path, file.name)
    rmSync(filePath, { recursive: true })
    file = dir.readSync()
  }
  dir.closeSync()
}