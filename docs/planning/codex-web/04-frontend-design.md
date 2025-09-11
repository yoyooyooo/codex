# 前端详细设计

**文档版本**: v1.0  
**最后更新**: 2025-09-11  
**依赖文档**: [02-architecture.md](02-architecture.md)  
**后续文档**: [06-api-specification.md](06-api-specification.md)

## 项目结构

```
apps/codex-web-ui/
├── package.json
├── vite.config.ts
├── src/
│   ├── main.tsx              # 应用入口
│   ├── App.tsx               # 根组件
│   ├── components/           # 通用组件
│   │   ├── ui/              # 基础 UI 组件
│   │   ├── layout/          # 布局组件
│   │   ├── chat/            # 聊天相关组件
│   │   ├── diff/            # 代码 diff 组件
│   │   └── forms/           # 表单组件
│   ├── pages/               # 页面组件
│   │   ├── SessionPage.tsx  # 会话页面
│   │   ├── HistoryPage.tsx  # 历史页面
│   │   └── TrustPage.tsx    # 信任设置页面
│   ├── hooks/               # 自定义 Hooks
│   │   ├── useWebSocket.ts  # WebSocket 连接
│   │   ├── useSession.ts    # 会话管理
│   │   └── useEvents.ts     # 事件处理
│   ├── stores/              # 状态管理
│   │   ├── sessionStore.ts  # 会话状态
│   │   ├── uiStore.ts       # UI 状态
│   │   └── authStore.ts     # 认证状态
│   ├── protocol/            # 生成的类型文件
│   │   └── types.ts         # 从后端生成
│   ├── utils/               # 工具函数
│   └── styles/              # 样式文件
└── public/                  # 静态资源
```

## 技术栈选择

### 核心框架
- **React 18**: 并发特性，Suspense
- **TypeScript**: 类型安全
- **Vite**: 快速构建工具
- **TanStack Query**: 服务端状态管理
- **Zustand**: 客户端状态管理

### UI 组件库
```json
{
  "dependencies": {
    "react": "^18.2.0",
    "@tanstack/react-query": "^5.0.0",
    "zustand": "^4.4.0",
    "react-markdown": "^9.0.0",
    "highlight.js": "^11.9.0",
    "@radix-ui/react-dialog": "^1.0.0",
    "@radix-ui/react-toast": "^1.1.0",
    "lucide-react": "^0.300.0"
  }
}
```

## 核心组件设计

### 会话组件
```typescript
// SessionView 主组件
interface SessionViewProps {
  sessionId: string;
}

export const SessionView: React.FC<SessionViewProps> = ({ sessionId }) => {
  const { session, isConnected } = useSession(sessionId);
  const { events, sendSubmission } = useWebSocket(sessionId);
  
  return (
    <div className="session-container">
      <ChatArea events={events} onSubmit={sendSubmission} />
      <DiffPanel session={session} />
      <StatusBar connected={isConnected} />
    </div>
  );
};
```

### 聊天区域
```typescript
interface ChatAreaProps {
  events: Event[];
  onSubmit: (submission: Submission) => void;
}

const ChatArea: React.FC<ChatAreaProps> = ({ events, onSubmit }) => {
  const [input, setInput] = useState('');
  
  return (
    <div className="chat-area">
      <MessageList events={events} />
      <InputArea 
        value={input}
        onChange={setInput}
        onSubmit={onSubmit}
      />
    </div>
  );
};
```

## 状态管理

### 会话状态
```typescript
interface SessionState {
  currentSessionId: string | null;
  sessions: Map<string, SessionData>;
  createSession: (config: SessionConfig) => Promise<void>;
  setCurrentSession: (id: string) => void;
}

export const useSessionStore = create<SessionState>((set, get) => ({
  currentSessionId: null,
  sessions: new Map(),
  
  createSession: async (config) => {
    const response = await api.createSession(config);
    set(state => ({
      sessions: state.sessions.set(response.session_id, {
        id: response.session_id,
        config,
        events: [],
        status: 'connecting'
      })
    }));
  },
  
  setCurrentSession: (id) => set({ currentSessionId: id })
}));
```

### WebSocket 连接
```typescript
export const useWebSocket = (sessionId: string) => {
  const [socket, setSocket] = useState<WebSocket | null>(null);
  const [events, setEvents] = useState<Event[]>([]);
  const [isConnected, setIsConnected] = useState(false);
  
  useEffect(() => {
    const ws = new WebSocket(`ws://localhost:8080/api/sessions/${sessionId}/events`);
    
    ws.onopen = () => setIsConnected(true);
    ws.onclose = () => setIsConnected(false);
    ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.type === 'event') {
        setEvents(prev => [...prev, message.event]);
      }
    };
    
    setSocket(ws);
    return () => ws.close();
  }, [sessionId]);
  
  const sendSubmission = useCallback((submission: Submission) => {
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({
        type: 'submission',
        submission,
        timestamp: Date.now()
      }));
    }
  }, [socket]);
  
  return { events, sendSubmission, isConnected };
};
```

## 页面路由

```typescript
import { BrowserRouter, Routes, Route } from 'react-router-dom';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/session/:id" element={<SessionPage />} />
        <Route path="/history" element={<HistoryPage />} />
        <Route path="/trust" element={<TrustPage />} />
      </Routes>
    </BrowserRouter>
  );
}
```

## UI/UX 设计要点

### 响应式布局
- 桌面端：左侧边栏 + 主内容区
- 移动端：全屏模式，底部导航
- 支持面板可调节大小

### 主题系统
```css
:root {
  --bg-primary: #ffffff;
  --bg-secondary: #f8fafc;
  --text-primary: #1e293b;
  --text-secondary: #64748b;
  --accent: #3b82f6;
}

[data-theme="dark"] {
  --bg-primary: #0f172a;
  --bg-secondary: #1e293b;
  --text-primary: #f1f5f9;
  --text-secondary: #94a3b8;
}
```

### 代码高亮
```typescript
import hljs from 'highlight.js';
import 'highlight.js/styles/github-dark.css';

const CodeBlock: React.FC<{ code: string; language: string }> = ({ code, language }) => {
  const highlightedCode = hljs.highlight(code, { language }).value;
  
  return (
    <pre className="code-block">
      <code dangerouslySetInnerHTML={{ __html: highlightedCode }} />
    </pre>
  );
};
```

---
**变更记录**：
- v1.0 (2025-09-11): 初始版本，前端设计概要