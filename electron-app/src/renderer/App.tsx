import React, { useState } from 'react';
import ChatPage from './pages/ChatPage';
import CardPage from './pages/CardPage';
import SettingsDialog from './components/SettingsDialog';
import PromptPreview from './components/PromptPreview';

const App: React.FC = () => {
  const [tab, setTab] = useState<'chat' | 'card'>('chat');
  const [showSettings, setShowSettings] = useState(false);
  const [showPrompt, setShowPrompt] = useState(false);

  return (
    <div className="app">
      <header className="toolbar">
        <div className="tabs">
          <button className={`tab ${tab === 'card' ? 'active' : ''}`} onClick={() => setTab('card')}>📋 角色卡</button>
          <button className={`tab ${tab === 'chat' ? 'active' : ''}`} onClick={() => setTab('chat')}>💬 对话</button>
        </div>
        <div className="toolbar-right">
          <button className="tb-btn" onClick={() => setShowPrompt(true)}>📝 Prompt</button>
          <button className="tb-btn" onClick={() => setShowSettings(true)}>⚙ 设置</button>
        </div>
      </header>

      <main className="content">
        {tab === 'card' ? <CardPage /> : <ChatPage />}
      </main>

      <footer className="statusbar"><span>🟢 Z&S-AI v0.1</span></footer>

      {showSettings && <SettingsDialog onClose={() => setShowSettings(false)} />}
      {showPrompt && <PromptPreview onClose={() => setShowPrompt(false)} />}
    </div>
  );
};

export default App;
