import "./styles.css";
import * as React from "react";
import { createRoot } from "react-dom/client";
import { Plus, Settings, Trash2, ChevronDown, Search } from "lucide-react";
import { Button } from "./components/ui/button.js";
import { Badge } from "./components/ui/badge.js";
import { Input } from "./components/ui/input.js";
import { Textarea } from "./components/ui/textarea.js";
import { Switch } from "./components/ui/switch.js";
import { Checkbox } from "./components/ui/checkbox.js";
import { RadioGroup, RadioGroupItem } from "./components/ui/radio-group.js";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs.js";
import { Progress } from "./components/ui/progress.js";
import { Alert, AlertTitle, AlertDescription } from "./components/ui/alert.js";
import { Card, CardHeader, CardTitle, CardDescription, CardContent, CardFooter } from "./components/ui/card.js";
import { Separator } from "./components/ui/separator.js";
import { Kbd, KbdGroup } from "./components/ui/kbd.js";
import { Label } from "./components/ui/label.js";
import { Spinner } from "./components/ui/spinner.js";
import { ButtonGroup } from "./components/ui/button-group.js";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./components/ui/tooltip.js";
import { Popover, PopoverContent, PopoverTrigger, PopoverHeader, PopoverTitle, PopoverDescription } from "./components/ui/popover.js";
import {
  DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator,
  DropdownMenuCheckboxItem, DropdownMenuShortcut, DropdownMenuSub, DropdownMenuSubTrigger, DropdownMenuSubContent,
} from "./components/ui/dropdown-menu.js";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectItem, SelectGroup, SelectLabel } from "./components/ui/select.js";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from "./components/ui/dialog.js";
import { AlertDialog, AlertDialogContent, AlertDialogHeader, AlertDialogTitle, AlertDialogDescription, AlertDialogFooter, AlertDialogAction, AlertDialogCancel } from "./components/ui/alert-dialog.js";
import { toast } from "./components/ui/toast.js";

const q = new URLSearchParams(location.search);
const theme = q.get("theme") === "dark" ? "zai-dark" : "zai-light";
const sheet = q.get("sheet") ?? "controls";
const root = document.documentElement;
root.classList.toggle("dark", theme === "zai-dark");
root.classList.toggle("theme-zai-dark", theme === "zai-dark");
root.classList.toggle("theme-zai-light", theme === "zai-light");
root.style.colorScheme = theme === "zai-dark" ? "dark" : "light";
// ZCode's desktop window frame is a transparent body over a window-root surface; this harness paints that surface itself.
document.body.style.background = "var(--color-background)";
document.body.style.color = "var(--color-foreground)";
document.body.style.font = "";
document.body.className = "font-sans text-ui-base";

const VARIANTS = ["default", "outline", "secondary", "ghost", "destructive", "warning", "link"] as const;
const SIZES = ["xs", "sm", "default", "lg"] as const;
const ICON_SIZES = ["icon-xs", "icon-sm", "icon", "icon-md", "icon-lg"] as const;

function Spec({ id, children, className }: { id: string; children: React.ReactNode; className?: string }) {
  return (
    <section data-specimen={id} className={className ?? "mb-6"}>
      <h2 className="mb-2 text-ui-sm text-foreground-subtle">{id}</h2>
      {children}
    </section>
  );
}

function Controls() {
  return (
    <div className="p-6">
      <Spec id="button-matrix">
        <div className="flex flex-col gap-2">
          {VARIANTS.map((v) => (
            <div key={v} className="flex items-center gap-2" data-row={v}>
              {SIZES.map((s) => (
                <Button key={s} variant={v} size={s}>
                  {v}/{s}
                </Button>
              ))}
              <Button variant={v}>
                <Plus data-icon="inline-start" /> With icon
              </Button>
              <Button variant={v} disabled>
                disabled
              </Button>
            </div>
          ))}
          <div className="flex items-center gap-2" data-row="icon">
            {ICON_SIZES.map((s) => (
              <Button key={s} variant="ghost" size={s} aria-label={s}>
                <Settings />
              </Button>
            ))}
            {ICON_SIZES.map((s) => (
              <Button key={s} variant="outline" size={s} aria-label={s}>
                <Trash2 />
              </Button>
            ))}
          </div>
        </div>
      </Spec>
      <Spec id="badge">
        <div className="flex gap-2">
          {(["default", "secondary", "destructive", "outline", "ghost", "link"] as const).map((v) => (
            <Badge key={v} variant={v}>
              {v}
            </Badge>
          ))}
        </div>
      </Spec>
      <Spec id="fields">
        <div className="flex max-w-md flex-col gap-3">
          <Label htmlFor="i1">Label</Label>
          <Input id="i1" placeholder="Placeholder" />
          <Input defaultValue="Filled value" />
          <Input disabled defaultValue="Disabled" />
          <Input aria-invalid defaultValue="Invalid" />
          <Textarea placeholder="Textarea placeholder" />
          <div className="flex items-center gap-3">
            <Switch defaultChecked aria-label="on" />
            <Switch aria-label="off" />
            <Checkbox defaultChecked aria-label="checked" />
            <Checkbox aria-label="unchecked" />
            <RadioGroup defaultValue="a" className="flex gap-3">
              <RadioGroupItem value="a" aria-label="ra" />
              <RadioGroupItem value="b" aria-label="rb" />
            </RadioGroup>
          </div>
        </div>
      </Spec>
      <Spec id="tabs">
        <Tabs defaultValue="one" className="max-w-md">
          <TabsList>
            <TabsTrigger value="one">One</TabsTrigger>
            <TabsTrigger value="two">Two</TabsTrigger>
            <TabsTrigger value="three" disabled>
              Three
            </TabsTrigger>
          </TabsList>
          <TabsContent value="one">Tab content one</TabsContent>
        </Tabs>
      </Spec>
      <Spec id="surfaces">
        <div className="flex max-w-3xl flex-col gap-3">
          <Card>
            <CardHeader>
              <CardTitle>Card title</CardTitle>
              <CardDescription>Card description text</CardDescription>
            </CardHeader>
            <CardContent>Card content in text-ui-base</CardContent>
            <CardFooter>
              <Button size="sm">Action</Button>
            </CardFooter>
          </Card>
          <Alert>
            <Search />
            <AlertTitle>Alert title</AlertTitle>
            <AlertDescription>Alert description</AlertDescription>
          </Alert>
          <Alert variant="warning">
            <AlertTitle>Warning alert</AlertTitle>
            <AlertDescription>Warning description</AlertDescription>
          </Alert>
          <Alert variant="destructive">
            <AlertTitle>Destructive alert</AlertTitle>
            <AlertDescription>Destructive description</AlertDescription>
          </Alert>
          <Progress value={42} />
          <Separator />
          <div className="flex items-center gap-2">
            <Spinner />
            <KbdGroup>
              <Kbd>⌘</Kbd>
              <Kbd>K</Kbd>
            </KbdGroup>
            <ButtonGroup>
              <Button variant="outline">Left</Button>
              <Button variant="outline">Right</Button>
            </ButtonGroup>
          </div>
        </div>
      </Spec>
    </div>
  );
}

const Trigger = React.forwardRef<HTMLButtonElement, React.ComponentProps<typeof Button>>(({ children, ...props }, ref) => (
  <Button ref={ref} variant="outline" {...props}>
    {children}
    <ChevronDown />
  </Button>
));

function Menu() {
  return (
    <div className="p-6" style={{ minHeight: 420 }}>
      <DropdownMenu open modal={false}>
        <DropdownMenuTrigger asChild>
          <Trigger>Menu</Trigger>
        </DropdownMenuTrigger>
        <DropdownMenuContent data-specimen="dropdown-menu" align="start">
          <DropdownMenuLabel>Label</DropdownMenuLabel>
          <DropdownMenuItem>
            New task <DropdownMenuShortcut>⌘N</DropdownMenuShortcut>
          </DropdownMenuItem>
          <DropdownMenuItem>Rename</DropdownMenuItem>
          <DropdownMenuItem disabled>Disabled</DropdownMenuItem>
          <DropdownMenuCheckboxItem checked>Checked item</DropdownMenuCheckboxItem>
          <DropdownMenuSeparator />
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>More</DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuItem>Sub item</DropdownMenuItem>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          <DropdownMenuItem variant="destructive">Delete</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}

function SelectSheet() {
  return (
    <div className="p-6" style={{ minHeight: 420 }}>
      <Select open defaultValue="b">
        <SelectTrigger data-specimen="select-trigger" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent data-specimen="select-content">
          <SelectGroup>
            <SelectLabel>Models</SelectLabel>
            <SelectItem value="a">Alpha</SelectItem>
            <SelectItem value="b">Beta</SelectItem>
            <SelectItem value="c">Gamma</SelectItem>
          </SelectGroup>
        </SelectContent>
      </Select>
    </div>
  );
}

function PopoverSheet() {
  return (
    <div className="p-6" style={{ minHeight: 320 }}>
      <Popover open>
        <PopoverTrigger asChild>
          <Trigger>Popover</Trigger>
        </PopoverTrigger>
        <PopoverContent data-specimen="popover" align="start">
          <PopoverHeader>
            <PopoverTitle>Popover title</PopoverTitle>
            <PopoverDescription>Popover description text</PopoverDescription>
          </PopoverHeader>
        </PopoverContent>
      </Popover>
    </div>
  );
}

function TooltipSheet() {
  return (
    <div className="p-16">
      <TooltipProvider>
        <Tooltip open>
          <TooltipTrigger asChild>
            <Button variant="ghost" size="icon" aria-label="t">
              <Settings />
            </Button>
          </TooltipTrigger>
          <TooltipContent data-specimen="tooltip" side="bottom">
            Tooltip text <Kbd>⌘K</Kbd>
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </div>
  );
}

function DialogSheet() {
  return (
    <Dialog open modal={false}>
      <DialogContent data-specimen="dialog" className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Dialog title</DialogTitle>
          <DialogDescription>Dialog description text that explains the action.</DialogDescription>
        </DialogHeader>
        <Input placeholder="Field" />
        <DialogFooter>
          <Button variant="outline">Cancel</Button>
          <Button>Confirm</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function AlertDialogSheet() {
  return (
    <AlertDialog open>
      <AlertDialogContent data-specimen="alert-dialog" className="sm:max-w-sm">
        <AlertDialogHeader>
          <AlertDialogTitle>Delete this task?</AlertDialogTitle>
          <AlertDialogDescription>This cannot be undone.</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction>Delete</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function ToastSheet() {
  React.useEffect(() => {
    toast("Saved the change", { position: "top-center" });
    toast("Something failed", { position: "top-center", variant: "destructive" } as never);
  }, []);
  return <div className="p-6" data-specimen="toast-host" style={{ minHeight: 320 }} />;
}

const SHEETS: Record<string, React.ReactNode> = {
  controls: <Controls />,
  menu: <Menu />,
  select: <SelectSheet />,
  popover: <PopoverSheet />,
  tooltip: <TooltipSheet />,
  dialog: <DialogSheet />,
  "alert-dialog": <AlertDialogSheet />,
  toast: <ToastSheet />,
};

createRoot(document.getElementById("root")!).render(<React.StrictMode>{SHEETS[sheet]}</React.StrictMode>);
