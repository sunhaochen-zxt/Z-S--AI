import React, { useState, useEffect } from 'react';
import { getCard } from '../api/client';

const FIELDS = [
  { key: 'name', label: '名称', rows: 1 },
  { key: 'description', label: '描述', rows: 3 },
  { key: 'personality', label: '性格', rows: 3 },
  { key: 'scenario', label: '场景', rows: 2 },
  { key: 'first_mes', label: '首次问候', rows: 2 },
  { key: 'example_dialogue', label: '示例对话', rows: 3 },
  { key: 'system_prompt', label: 'System Prompt', rows: 3 },
  { key: 'creator_notes', label: '创作者备注', rows: 2 },
];

const CardPage: React.FC = () => {
  const [card, setCard] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getCard().then(data => {
      if (data && !data.error) {
        setCard(data);
      }
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  const update = (key: string, value: string) => {
    setCard(prev => ({ ...prev, [key]: value }));
  };

  if (loading) return <div className="card-page"><p>加载中…</p></div>;

  return (
    <div className="card-page">
      <div className="card-toolbar">
        <span style={{ flex: 1, fontSize: 18, fontWeight: 600 }}>
          {card.name || '(未命名)'}
        </span>
        <button onClick={() => getCard().then(d => { if (d && !d.error) setCard(d); })}>
          🔄 刷新
        </button>
      </div>

      {FIELDS.map(f => (
        <div key={f.key} className="card-field">
          <label>{f.label}</label>
          {f.rows > 1 ? (
            <textarea
              rows={f.rows}
              value={card[f.key] || ''}
              onChange={e => update(f.key, e.target.value)}
            />
          ) : (
            <input
              value={card[f.key] || ''}
              onChange={e => update(f.key, e.target.value)}
            />
          )}
        </div>
      ))}
    </div>
  );
};

export default CardPage;
