// 后端 API 客户端
let BASE_URL = 'http://127.0.0.1:9786';
let WS_URL = 'ws://127.0.0.1:9786/ws/chat';

/** 设置后端地址（Electron 主进程检测到端口后调用） */
export function setBaseUrl(port: number): void {
  BASE_URL = `http://127.0.0.1:${port}`;
  WS_URL = `ws://127.0.0.1:${port}/ws/chat`;
}

export async function api<T = any>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...(options?.headers as Record<string, string>),
    },
  });
  return res.json();
}

export function chat(message: string): Promise<{ reply?: string; error?: string; usage?: any }> {
  return api('/api/chat', {
    method: 'POST',
    body: JSON.stringify({ message }),
  });
}

export function getCard(): Promise<Record<string, string>> {
  return api('/api/card');
}

export function getConfig(): Promise<any> {
  return api('/api/config');
}

export function getHistory(sessionId?: string): Promise<any> {
  const q = sessionId ? `?session_id=${sessionId}` : '';
  return api(`/api/history${q}`);
}

export function createSession(character?: string, model?: string): Promise<any> {
  return api('/api/session', {
    method: 'POST',
    body: JSON.stringify({ character, model }),
  });
}

export function getPromptPreview(): Promise<{ prompt: string; prompt_length: number; estimated_tokens: number; character: any }> {
  return api('/api/prompt/preview', { method: 'POST' });
}

export function exportHistory(sessionId: string, format: 'json' | 'markdown' = 'json'): Promise<any> {
  return api(`/api/history/export?session_id=${sessionId}&format=${format}`);
}

// ── WebSocket 流式连接 ──

export interface StreamCallbacks {
  onToken: (fullText: string) => void;
  onDone: () => void;
  onError: (message: string) => void;
}

/** 建立 WebSocket 流式连接，返回取消函数 */
export function connectStream(message: string, cb: StreamCallbacks): () => void {
  const ws = new WebSocket(WS_URL);

  ws.onopen = () => ws.send(JSON.stringify({ message }));

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data as string);
      switch (data.type) {
        case 'partial': cb.onToken(data.content); break;
        case 'done': cb.onDone(); ws.close(); break;
        case 'error': cb.onError(data.message || '未知错误'); ws.close(); break;
        default: cb.onError(`未知消息类型: ${data.type}`); ws.close();
      }
    } catch {
      cb.onError('消息解析失败');
    }
  };

  ws.onerror = () => cb.onError('WebSocket 连接失败，请确认后端已启动');

  return () => ws.close();
}
