import React, { useState, useEffect } from 'react';
import { getConfig, api } from '../api/client';

interface Props { onClose: () => void; }

const SettingsDialog: React.FC<Props> = ({ onClose }) => {
  const [apiType, setApiType] = useState('deepseek');
  const [apiKey, setApiKey] = useState('');
  const [baseUrl, setBaseUrl] = useState('https://api.deepseek.com');
  const [model, setModel] = useState('deepseek-v4-flash');
  const [stream, setStream] = useState(false);
  const [keyConfigured, setKeyConfigured] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [msg, setMsg] = useState('');

  useEffect(() => {
    getConfig().then((data: any) => {
      if (data?.api) {
        setApiType(data.api.api_type || 'deepseek');
        setBaseUrl(data.api.base_url || 'https://api.deepseek.com');
        setModel(data.api.model || 'deepseek-v4-flash');
        setStream(data.api.stream || false);
        setKeyConfigured(data.api.api_key_configured || false);
      }
      setLoaded(true);
    }).catch(() => setLoaded(true));
  }, []);

  const save = async () => {
    setSaving(true);
    setMsg('');
    try {
      await api('/api/config', {
        method: 'PUT',
        body: JSON.stringify({
          api_type: apiType,
          api_key: apiKey,
          base_url: baseUrl,
          model,
          stream,
        }),
      });
      setMsg('✅ 已保存。API Key 已更新（当前会话生效）');
      setKeyConfigured(!!apiKey);
    } catch {
      setMsg('❌ 保存失败，请确认后端已启动');
    }
    setSaving(false);
  };

  return (
    <div className="dialog-overlay" onClick={onClose}>
      <div className="dialog" onClick={e => e.stopPropagation()}>
        <h3>⚙ API 设置</h3>
        {!loaded ? <p>加载中…</p> : (
          <>
            <div className="dialog-field">
              <label>API 类型</label>
              <select value={apiType} onChange={e => setApiType(e.target.value)}>
                <option value="deepseek">DeepSeek</option>
                <option value="openai">OpenAI 兼容</option>
              </select>
            </div>
            <div className="dialog-field">
              <label>API Key {keyConfigured && <span style={{color:'green'}}>✅ 已配置</span>}</label>
              <input
                type="password"
                value={apiKey}
                onChange={e => setApiKey(e.target.value)}
                placeholder={keyConfigured ? '••••••••（留空不修改）' : 'sk-xxxxxxxx'}
              />
            </div>
            <div className="dialog-field">
              <label>Base URL</label>
              <input value={baseUrl} onChange={e => setBaseUrl(e.target.value)} />
            </div>
            <div className="dialog-field">
              <label>模型</label>
              <input value={model} onChange={e => setModel(e.target.value)} />
            </div>
            <div className="dialog-field">
              <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
                <input type="checkbox" checked={stream} onChange={e => setStream(e.target.checked)} />
                默认启用流式响应
              </label>
            </div>
            {msg && <p style={{ fontSize: 13, marginTop: 8, color: msg.startsWith('✅') ? 'green' : 'red' }}>{msg}</p>}
          </>
        )}
        <div className="dialog-actions">
          <button className="btn-cancel" onClick={onClose}>关闭</button>
          <button className="btn-ok" onClick={save} disabled={saving}>
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </div>
    </div>
  );
};

export default SettingsDialog;
