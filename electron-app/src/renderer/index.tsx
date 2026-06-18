import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import './App.css';

// 后端端口固定 9786（Electron 主进程会先清理端口再 spawn）
console.log('[ZS-AI] 前端已加载，后端端口: 9786');

const root = createRoot(document.getElementById('root')!);
root.render(<React.StrictMode><App /></React.StrictMode>);
