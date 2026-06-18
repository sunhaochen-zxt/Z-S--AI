import React, { useState, useEffect } from 'react';
import { getPromptPreview } from '../api/client';

interface Props { onClose: () => void; }

const PromptPreview: React.FC<Props> = ({ onClose }) => {
  const [prompt, setPrompt] = useState('');
  const [tokens, setTokens] = useState(0);
  const [length, setLength] = useState(0);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getPromptPreview().then(data => {
      setPrompt(data.prompt || '');
      setTokens(data.estimated_tokens || 0);
      setLength(data.prompt_length || 0);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  return (
    <div className="dialog-overlay" onClick={onClose}>
      <div className="dialog" onClick={e => e.stopPropagation()} style={{ maxWidth: 720, width: '90vw' }}>
        <h3>📝 System Prompt 预览</h3>
        {loading ? <p>加载中…</p> : (
          <>
            <div style={{ display: 'flex', gap: 16, marginBottom: 12, fontSize: 13, color: 'var(--on-surface-variant)' }}>
              <span>字符数: {length.toLocaleString()}</span>
              <span>估算 Token: ~{tokens.toLocaleString()}</span>
            </div>
            <pre style={{
              background: '#f5f5f5', padding: 16, borderRadius: 8,
              maxHeight: 400, overflow: 'auto', fontSize: 12,
              whiteSpace: 'pre-wrap', wordBreak: 'break-word',
              fontFamily: 'monospace',
            }}>
              {prompt}
            </pre>
          </>
        )}
        <div className="dialog-actions" style={{ marginTop: 16 }}>
          <button className="btn-cancel" onClick={onClose}>关闭</button>
        </div>
      </div>
    </div>
  );
};

export default PromptPreview;
