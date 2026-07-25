import type { SVGProps } from 'react';
import {
  Accessibility,
  Archive,
  ArrowLeft,
  ArrowRight,
  BrowserCode,
  ChatSquare,
  CheckSquare,
  ChevronDown,
  CloseSquare,
  CodeFile,
  Cpu,
  Envelope,
  FileText,
  Files,
  Folder,
  FolderAdd,
  Globe,
  IconComponent,
  InfoSquare,
  MagicWand,
  Maximize,
  Minimize,
  Moon,
  MoreSquare,
  Palette,
  PenSquare,
  Pin,
  Plus,
  Refresh,
  Scan,
  Search,
  Pointer,
  Settings,
  Shield,
  SidebarLeft,
  SidebarRight,
  Sliders,
  Stop,
  Sun,
  TaskSquare,
  TerminalSquare,
  ThreeDCube,
  Trash,
  Warning,
} from 'reicon-react';

export type IconName =
  | 'sidebar'
  | 'search'
  | 'plus'
  | 'compose'
  | 'folder'
  | 'chat'
  | 'capabilities'
  | 'mail'
  | 'artifact'
  | 'archive'
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
  compose: PenSquare,
  folder: Folder,
  chat: ChatSquare,
  capabilities: ThreeDCube,
  mail: Envelope,
  artifact: FileText,
  archive: Archive,
  browser: BrowserCode,
  files: Files,
  terminal: TerminalSquare,
  tasks: TaskSquare,
  settings: Settings,
  sun: Sun,
  moon: Moon,
  panel: SidebarRight,
  back: ArrowLeft,
  forward: ArrowRight,
  reload: Refresh,
  send: Pointer,
  stop: Stop,
  chevron: ChevronDown,
  pin: Pin,
  trash: Trash,
  refresh: Refresh,
  check: CheckSquare,
  warning: Warning,
  close: CloseSquare,
  more: MoreSquare,
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
  info: InfoSquare,
};

export function Icon({ name, ...props }: SVGProps<SVGSVGElement> & { name: IconName }) {
  const ReiconIcon = icons[name];
  const className = ['optimus-icon', props.className].filter(Boolean).join(' ');

  return <ReiconIcon aria-hidden="true" weight="Outline" {...props} className={className} />;
}
