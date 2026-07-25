import type { SVGProps } from 'react';
import {
  Accessibility,
  ArrowLeft,
  ArrowRight,
  BrowserCode,
  ChatRound,
  Check,
  ChevronDown,
  CloseCircle,
  CodeFile,
  Cpu,
  Envelope,
  FileText,
  Files,
  Folder,
  FolderAdd,
  Globe,
  IconComponent,
  InfoCircle,
  MagicWand,
  Maximize,
  Minimize,
  Moon,
  More,
  Palette,
  Pin,
  Plus,
  Refresh,
  Scan,
  Search,
  Send,
  Settings,
  Shield,
  SidebarLeft,
  SidebarRight,
  Sliders,
  Stop,
  Sun,
  Task,
  TerminalSquare,
  ThreeDCube,
  Trash,
  Warning,
} from 'reicon-react';

export type IconName =
  | 'sidebar'
  | 'search'
  | 'plus'
  | 'folder'
  | 'chat'
  | 'capabilities'
  | 'mail'
  | 'artifact'
  | 'browser'
  | 'files'
  | 'terminal'
  | 'tasks'
  | 'settings'
  | 'sun'
  | 'moon'
  | 'panel'
  | 'back'
  | 'forward'
  | 'reload'
  | 'send'
  | 'stop'
  | 'chevron'
  | 'pin'
  | 'trash'
  | 'refresh'
  | 'check'
  | 'warning'
  | 'close'
  | 'more'
  | 'annotation'
  | 'minimize'
  | 'maximize'
  | 'project'
  | 'appearance'
  | 'agent'
  | 'shield'
  | 'memory'
  | 'automation'
  | 'accessibility'
  | 'advanced'
  | 'globe'
  | 'source'
  | 'info';

// Keep the application's semantic icon vocabulary stable while sourcing every
// rendered glyph from Reicon. Their Outline components use currentColor, so
// the existing CSS hex tokens remain the single source of icon colour.
const icons: Record<IconName, IconComponent> = {
  sidebar: SidebarLeft,
  search: Search,
  plus: Plus,
  folder: Folder,
  chat: ChatRound,
  capabilities: ThreeDCube,
  mail: Envelope,
  artifact: FileText,
  browser: BrowserCode,
  files: Files,
  terminal: TerminalSquare,
  tasks: Task,
  settings: Settings,
  sun: Sun,
  moon: Moon,
  panel: SidebarRight,
  back: ArrowLeft,
  forward: ArrowRight,
  reload: Refresh,
  send: Send,
  stop: Stop,
  chevron: ChevronDown,
  pin: Pin,
  trash: Trash,
  refresh: Refresh,
  check: Check,
  warning: Warning,
  close: CloseCircle,
  more: More,
  annotation: Scan,
  minimize: Minimize,
  maximize: Maximize,
  project: FolderAdd,
  appearance: Palette,
  agent: Cpu,
  shield: Shield,
  memory: Cpu,
  automation: MagicWand,
  accessibility: Accessibility,
  advanced: Sliders,
  globe: Globe,
  source: CodeFile,
  info: InfoCircle,
};

export function Icon({ name, ...props }: SVGProps<SVGSVGElement> & { name: IconName }) {
  const ReiconIcon = icons[name];

  return <ReiconIcon aria-hidden="true" weight={name === 'more' ? 'Filled' : 'Outline'} {...props} />;
}
