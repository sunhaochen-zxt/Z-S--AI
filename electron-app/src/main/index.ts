// Electron 主进程
import { app, BrowserWindow } from 'electron';
import { spawn, ChildProcess } from 'child_process';
import { join } from 'path';
import { existsSync } from 'fs';

// 路径：开发模式用项目目录，生产模式用 electron-builder 打包的 resources
const isDev = !app.isPackaged;
const PROJECT_ROOT = isDev ? join(__dirname, '..', '..', '..') : process.resourcesPath;
const BIN_DIR = isDev ? join(PROJECT_ROOT, 'electron-app', 'bin') : join(process.resourcesPath, 'bin');
const SERVER_PATH = join(BIN_DIR, 'zsai-server');
const RENDERER_DIST = join(__dirname, '..', '..', 'dist-renderer', 'index.html');
const CONFIG_PATH = join(BIN_DIR, 'config.toml');

let mainWindow: BrowserWindow | null = null;
let backendProcess: ChildProcess | null = null;
let backendPort = 9786;

function startBackend(): Promise<number> {
  return new Promise((resolve) => {
    if (!existsSync(SERVER_PATH)) {
      console.warn('后端二进制不存在:', SERVER_PATH, '\n请先执行: cargo build --workspace');
      resolve(backendPort);
      return;
    }

    // 先清理可能占用的端口
    try { require('child_process').execSync('fuser -k 9786/tcp 2>/dev/null || true'); } catch (_) {}

    backendProcess = spawn(SERVER_PATH, [CONFIG_PATH], {
      env: { ...process.env },
      cwd: BIN_DIR,  // 工作目录设为 bin/，data/ 等相对路径都从这里解析
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let started = false;
    backendProcess.stdout?.on('data', (data: Buffer) => {
      const line = data.toString().trim();
      console.log('[backend]', line);
      if (line.startsWith('LISTENING=') && !started) {
        started = true;
        backendPort = parseInt(line.split('=')[1], 10);
        resolve(backendPort);
      }
    });

    backendProcess.stderr?.on('data', (data: Buffer) => {
      console.error('[backend-err]', data.toString().trim());
    });

    backendProcess.on('exit', (code) => {
      console.log(`后端进程退出，退出码: ${code}`);
      if (code !== 0 && !started) {
        resolve(backendPort);
      }
    });

    // 超时回退
    setTimeout(() => { if (!started) resolve(backendPort); }, 5000);
  });
}

function createWindow(port: number) {
  mainWindow = new BrowserWindow({
    width: 1100,
    height: 720,
    title: 'Z&S-AI',
    webPreferences: {
      preload: join(__dirname, '..', 'preload', 'index.js'),
      contextIsolation: true,
      nodeIntegration: false,
    },
  });

  // 生产模式：加载打包后的 renderer
  if (existsSync(RENDERER_DIST)) {
    console.log('加载生产模式:', RENDERER_DIST);
    mainWindow.loadFile(RENDERER_DIST);
  } else {
    // 开发模式：尝试 Vite dev server
    console.log('尝试开发模式: http://localhost:5173');
    mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools({ mode: 'detach' });
  }

  // 页面加载完成后发送端口号
  mainWindow.webContents.on('did-finish-load', () => {
    mainWindow?.webContents.send('backend-port', port);
  });
}

app.whenReady().then(async () => {
  const port = await startBackend();
  createWindow(port);
});

app.on('window-all-closed', () => {
  if (backendProcess) {
    backendProcess.kill('SIGTERM');
    setTimeout(() => { backendProcess?.kill('SIGKILL'); }, 5000);
  }
  app.quit();
});
