import type { ReactNode, SVGProps } from 'react';

export type IconName =
  | 'sidebar'
  | 'search'
  | 'plus'
  | 'folder'
  | 'chat'
  | 'capabilities'
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
  | 'more';

const paths: Record<IconName, ReactNode> = {
  sidebar: <path d="M3.5 4.5h17v15h-17zM8 4.5v15" />,
  search: <><circle cx="10.5" cy="10.5" r="5.5" /><path d="m15 15 4.5 4.5" /></>,
  plus: <path d="M12 5v14M5 12h14" />,
  folder: <path d="M3.5 6.5h6l2 2h9v10h-17z" />,
  chat: <path d="M4 5.5h16v11H9l-5 3z" />,
  capabilities: <><path d="M12 3.5 19 7.5v9L12 20.5 5 16.5v-9z" /><path d="m8.5 12 2.2 2.2 4.8-5" /></>,
  artifact: <><path d="M6 3.5h8l4 4v13H6z" /><path d="M14 3.5v4h4M9 12h6M9 16h5" /></>,
  browser: <><rect x="3.5" y="4.5" width="17" height="15" rx="2" /><path d="M3.5 8.5h17M7 6.5h.1M10 6.5h.1" /></>,
  files: <><path d="M5 3.5h9l5 5v12H5z" /><path d="M14 3.5v5h5" /></>,
  terminal: <><rect x="3.5" y="4.5" width="17" height="15" rx="2" /><path d="m7 9 3 3-3 3M12.5 15H17" /></>,
  tasks: <><path d="M7 5h13M7 12h13M7 19h13" /><path d="m3 5 .8.8L5.5 4M3 12l.8.8L5.5 11M3 19l.8.8 1.7-1.8" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M12 3.5v2M12 18.5v2M3.5 12h2M18.5 12h2M6 6l1.5 1.5M16.5 16.5 18 18M18 6l-1.5 1.5M7.5 16.5 6 18" /></>,
  sun: <><circle cx="12" cy="12" r="4" /><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M18.7 5.3l-1.4 1.4M6.7 17.3l-1.4 1.4" /></>,
  moon: <path d="M19 15.5A8 8 0 0 1 8.5 5a8.5 8.5 0 1 0 10.5 10.5Z" />,
  panel: <path d="M4 4.5h16v15H4zM15 4.5v15" />,
  back: <path d="m14.5 6-6 6 6 6" />,
  forward: <path d="m9.5 6 6 6-6 6" />,
  reload: <><path d="M19 8a8 8 0 1 0 1 6" /><path d="M19 3.5V8h-4.5" /></>,
  send: <path d="m4 5 16 7-16 7 3-7zM7 12h13" />,
  stop: <rect x="7" y="7" width="10" height="10" rx="2" />,
  chevron: <path d="m8 10 4 4 4-4" />,
  pin: <><path d="m9 4 6 2-1 5 3 3-5 1-3 5 .5-6-3-3z" /><path d="m9 15-4 4" /></>,
  trash: <><path d="M5 7h14M9 3.5h6L16 7H8zM7 7l1 13h8l1-13" /><path d="M10 10v6M14 10v6" /></>,
  refresh: <><path d="M19 8a8 8 0 1 0 1 6" /><path d="M19 3.5V8h-4.5" /></>,
  check: <path d="m5 12 4 4 10-10" />,
  warning: <><path d="M12 3.5 21 20H3z" /><path d="M12 9v5M12 17.5h.1" /></>,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  more: <><circle cx="5" cy="12" r="1" fill="currentColor" /><circle cx="12" cy="12" r="1" fill="currentColor" /><circle cx="19" cy="12" r="1" fill="currentColor" /></>,
};

export function Icon({ name, ...props }: SVGProps<SVGSVGElement> & { name: IconName }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      {paths[name]}
    </svg>
  );
}
