import React, { useState, useRef, useEffect } from 'react';
import { chat as apiChat, connectStream } from '../api/client';

interface Message { role: 'user' | 'ai' | 'error'; content: string; }

const ChatPage: React.FC = () => {
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const [useStream, setUseStream] = useState(true);
  const messagesEnd = useRef<HTMLDivElement>(null);
  const streamIdx = useRef(-1);

  useEffect(() => { messagesEnd.current?.scrollIntoView({ behavior: 'smooth' }); }, [messages]);

  const add = (role: Message['role'], content: string) =>
    setMessages(prev => [...prev, { role, content }]);

  const doSend = () => {
    const text = input.trim();
    if (!text || busy) return;
    add('user', text);
    setInput('');
    setBusy(true);

    if (useStream) {
      setStreaming(true);
      streamIdx.current = -1;
      connectStream(text, {
        onToken(full) {
          setMessages(prev => {
            const copy = [...prev];
            if (streamIdx.current < 0) {
              copy.push({ role: 'ai', content: full });
              streamIdx.current = copy.length - 1;
            } else {
              copy[streamIdx.current] = { role: 'ai', content: full };
            }
            return copy;
          });
        },
        onDone() {
          setStreaming(false); setBusy(false); streamIdx.current = -1;
        },
        onError(err) {
          add('error', err);
          setStreaming(false); setBusy(false);
        },
      });
    } else {
      apiChat(text).then(data => {
        if (data.reply) add('ai', data.reply);
        if (data.error) add('error', data.error);
        setBusy(false);
      }).catch(() => {
        add('error', '网络请求失败，请确认后端已启动');
        setBusy(false);
      });
    }
  };

  return (
    <div className="chat-page">
      <div style={{ marginBottom: 8, display: 'flex', gap: 8, alignItems: 'center' }}>
        <label style={{ fontSize: 13, display: 'flex', alignItems: 'center', gap: 4, cursor: 'pointer' }}>
          <input type="checkbox" checked={useStream} onChange={e => setUseStream(e.target.checked)} />
          流式显示
        </label>
      </div>
      <div className="chat-messages">
        {messages.map((m, i) => (
          <div key={i} className={`bubble ${m.role}`}>{m.content}</div>
        ))}
        <div ref={messagesEnd} />
      </div>
      <div className="chat-input-row">
        <input
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter') doSend(); }}
          placeholder="输入消息…（Enter 发送）"
          disabled={busy}
          autoFocus
        />
        <button onClick={doSend} disabled={busy || !input.trim()}>
          {busy ? (streaming ? '接收中…' : '发送中…') : '发送'}
        </button>
      </div>
    </div>
  );
};

export default ChatPage;
