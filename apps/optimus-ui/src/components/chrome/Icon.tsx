import type { CSSProperties } from 'react';

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

// Every glyph is a codepoint in the bundled 0xProto Nerd Font (Propo metrics).
// The application's semantic icon vocabulary stays stable; only the source of
// the glyph changed (Reicon SVGs → Nerd Font codepoints). Icons use
// currentColor, so the existing CSS hex tokens remain the colour source.
const GLYPHS: Record<IconName, string> = {
  sidebar: '\uEBF3', // cod-layout_sidebar_left
  search: '\uEA6D', // cod-search
  plus: '\uEA60', // cod-add
  compose: '\uEA73', // cod-edit
  folder: '\uEA83', // cod-folder
  chat: '\uEA6B', // cod-comment
  capabilities: '\uEB29', // cod-package
  mail: '\uEB1C', // cod-mail
  artifact: '\uEA7B', // cod-file
  archive: '\uEA98', // cod-archive
  browser: '\uEAAE', // cod-browser
  files: '\uEAF0', // cod-files
  terminal: '\uEA85', // cod-terminal
  tasks: '\uEAB3', // cod-checklist
  settings: '\uEAF8', // cod-gear
  sun: '\uF185', // fa-sun-o
  moon: '\uF186', // fa-moon-o
  panel: '\uEBF4', // cod-layout_sidebar_right
  back: '\uEA9B', // cod-arrow_left
  forward: '\uEA9C', // cod-arrow_right
  reload: '\uEB37', // cod-refresh
  send: '\uEC0F', // cod-send
  stop: '\uEABD', // cod-circle_slash
  chevron: '\uEAB4', // cod-chevron_down
  pin: '\uEB2B', // cod-pin
  trash: '\uEA81', // cod-trash
  refresh: '\uEB37', // cod-refresh
  check: '\uEAB2', // cod-check
  warning: '\uEA6C', // cod-warning
  close: '\uEA76', // cod-close
  more: '\uEA7C', // cod-ellipsis
  annotation: '\uEB26', // cod-note
  minimize: '\uEABA', // cod-chrome_minimize
  maximize: '\uEAB9', // cod-chrome_maximize
  project: '\uEAAC', // cod-briefcase
  appearance: '\uEFCC', // fa-palette
  agent: '\uEC20', // cod-robot
  shield: '\uEB53', // cod-shield
  memory: '\uEACE', // cod-database
  automation: '\uEBCF', // cod-wand
  accessibility: '\uF29A', // fa-universal_access
  advanced: '\uF1DE', // fa-sliders
  globe: '\uEB01', // cod-globe
  source: '\uEAC4', // cod-code
  info: '\uEA74', // cod-info
};

export function Icon({
  name,
  className,
  style,
}: {
  name: IconName;
  className?: string;
  style?: CSSProperties;
}) {
  const classes = ['optimus-icon', className].filter(Boolean).join(' ');
  return (
    <span className={classes} style={style} aria-hidden="true" role="img" data-icon={name}>
      {GLYPHS[name]}
    </span>
  );
}
