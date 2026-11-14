// The one icon set (Lucide, ISC; ADR-012). Each icon is imported on its own so only used icons
// ship. Add an icon here before using it; the names follow the guideline's icon map (§6.1).

import ArrowLeft from "lucide-solid/icons/arrow-left";
import ArrowRight from "lucide-solid/icons/arrow-right";
import Bookmark from "lucide-solid/icons/bookmark";
import BookmarkPlus from "lucide-solid/icons/bookmark-plus";
import Check from "lucide-solid/icons/check";
import ChevronDown from "lucide-solid/icons/chevron-down";
import ChevronRight from "lucide-solid/icons/chevron-right";
import Circle from "lucide-solid/icons/circle";
import CircleCheck from "lucide-solid/icons/circle-check";
import CircleAlert from "lucide-solid/icons/circle-alert";
import Columns2 from "lucide-solid/icons/columns-2";
import Command from "lucide-solid/icons/command";
import Copy from "lucide-solid/icons/copy";
import Ellipsis from "lucide-solid/icons/ellipsis";
import Eraser from "lucide-solid/icons/eraser";
import ExternalLink from "lucide-solid/icons/external-link";
import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import FileDown from "lucide-solid/icons/file-down";
import FilePlus from "lucide-solid/icons/file-plus";
import Folder from "lucide-solid/icons/folder";
import Highlighter from "lucide-solid/icons/highlighter";
import History from "lucide-solid/icons/rotate-ccw-clock";
import Keyboard from "lucide-solid/icons/keyboard";
import LayoutGrid from "lucide-solid/icons/layout-grid";
import Library from "lucide-solid/icons/library";
import Link from "lucide-solid/icons/link";
import ListChecks from "lucide-solid/icons/list-checks";
import ListTree from "lucide-solid/icons/list-tree";
import Maximize2 from "lucide-solid/icons/maximize-2";
import MessageCircle from "lucide-solid/icons/message-circle";
import MessageCirclePlus from "lucide-solid/icons/message-circle-plus";
import MessageSquare from "lucide-solid/icons/message-square";
import MessagesSquare from "lucide-solid/icons/messages-square";
import Minimize2 from "lucide-solid/icons/minimize-2";
import Minus from "lucide-solid/icons/minus";
import MousePointer2 from "lucide-solid/icons/mouse-pointer-2";
import MoveUpRight from "lucide-solid/icons/move-up-right";
import NotebookPen from "lucide-solid/icons/notebook-pen";
import PanelLeft from "lucide-solid/icons/panel-left";
import PanelRight from "lucide-solid/icons/panel-right";
import PenLine from "lucide-solid/icons/pen-line";
import Plus from "lucide-solid/icons/plus";
import Printer from "lucide-solid/icons/printer";
import Quote from "lucide-solid/icons/quote";
import Redo2 from "lucide-solid/icons/redo-2";
import Search from "lucide-solid/icons/search";
import Settings from "lucide-solid/icons/settings";
import Sparkles from "lucide-solid/icons/sparkles";
import Spline from "lucide-solid/icons/spline";
import Square from "lucide-solid/icons/square";
import StickyNote from "lucide-solid/icons/sticky-note";
import Strikethrough from "lucide-solid/icons/strikethrough";
import SunMoon from "lucide-solid/icons/sun-moon";
import Tag from "lucide-solid/icons/tag";
import TextQuote from "lucide-solid/icons/text-quote";
import Trash from "lucide-solid/icons/trash";
import Type from "lucide-solid/icons/type";
import Underline from "lucide-solid/icons/underline";
import Undo2 from "lucide-solid/icons/undo-2";
import X from "lucide-solid/icons/x";
import ZoomIn from "lucide-solid/icons/zoom-in";
import ZoomOut from "lucide-solid/icons/zoom-out";

export const ICONS = {
  "arrow-left": ArrowLeft,
  "arrow-right": ArrowRight,
  bookmark: Bookmark,
  "bookmark-plus": BookmarkPlus,
  check: Check,
  "chevron-down": ChevronDown,
  "chevron-right": ChevronRight,
  circle: Circle,
  "circle-check": CircleCheck,
  "circle-alert": CircleAlert,
  "columns-2": Columns2,
  command: Command,
  copy: Copy,
  ellipsis: Ellipsis,
  eraser: Eraser,
  "external-link": ExternalLink,
  eye: Eye,
  "eye-off": EyeOff,
  "file-down": FileDown,
  "file-plus": FilePlus,
  folder: Folder,
  highlighter: Highlighter,
  history: History,
  keyboard: Keyboard,
  "layout-grid": LayoutGrid,
  library: Library,
  link: Link,
  "list-checks": ListChecks,
  "list-tree": ListTree,
  "maximize-2": Maximize2,
  "message-circle": MessageCircle,
  "message-circle-plus": MessageCirclePlus,
  "message-square": MessageSquare,
  "messages-square": MessagesSquare,
  "minimize-2": Minimize2,
  minus: Minus,
  "mouse-pointer-2": MousePointer2,
  "move-up-right": MoveUpRight,
  "notebook-pen": NotebookPen,
  "panel-left": PanelLeft,
  "panel-right": PanelRight,
  "pen-line": PenLine,
  plus: Plus,
  printer: Printer,
  quote: Quote,
  "redo-2": Redo2,
  search: Search,
  settings: Settings,
  sparkles: Sparkles,
  spline: Spline,
  square: Square,
  "sticky-note": StickyNote,
  strikethrough: Strikethrough,
  "sun-moon": SunMoon,
  tag: Tag,
  "text-quote": TextQuote,
  trash: Trash,
  type: Type,
  underline: Underline,
  "undo-2": Undo2,
  x: X,
  "zoom-in": ZoomIn,
  "zoom-out": ZoomOut,
} as const;

export type IconName = keyof typeof ICONS;
