# The toolbar the overlay shows before a capture exists.
toolbar-title = Crabsnap
toolbar-hint = Capture is not wired up yet

# The selection's action bar.
action-save = Save
action-copy = Copy
action-pin = Pin
action-cancel = Cancel
action-confirm = Confirm

# A pin's right-click menu.
pin-menu-copy = Copy
pin-menu-save = Save
pin-menu-reset-zoom = Actual size
pin-menu-edit = Edit
pin-menu-done = Done
pin-menu-close = Close

# The bar at a surface's bottom, after an action that left nothing else on screen.
notice-copied = Copied to the clipboard
notice-saved = Saved { $file }
notice-failed = { $action } failed

# The annotation session's tools and rows.
pin-edit-rect = Rect
pin-edit-ellipse = Ellipse
pin-edit-line = Line
pin-edit-arrow = Arrow
pin-edit-pen = Freehand
pin-edit-bezier = Bezier Pen
pin-edit-text = Text
pin-edit-edit-text = Edit Text
pin-edit-edit-points = Edit Points
pin-edit-fill = Fill
pin-edit-delete = Delete
pin-edit-finish-text = Finish Text
pin-edit-discard-text = Discard Changes
pin-edit-finish-path = Finish Path
pin-edit-discard-path = Discard Path
toolbar-cursor = Cursor
toolbar-loupe = Loupe
toolbar-loupe-temporary = Loupe (held with the right button)
toolbar-stroke = Stroke
toolbar-colour = Colour
toolbar-width = Width
toolbar-heads = Arrowheads
toolbar-head-end = At the end
toolbar-head-both = At both ends
toolbar-outline = Text outline
toolbar-border = Frame border
toolbar-size = Text size
toolbar-padding = Padding
toolbar-radius = Corner radius
toolbar-align = Alignment
toolbar-align-left = Left
toolbar-align-center = Center
toolbar-align-right = Right
toolbar-none = None
toolbar-pick = Pick a colour from the picture

# Palette pins.
pin-menu-copy-hex = Copy HEX
pin-menu-copy-rgb = Copy RGB
pin-menu-copy-hsl = Copy HSL
pin-menu-pick = Pick Colour
pin-menu-roles = MD3 Roles
pin-menu-add-colour = Add Colour
pin-menu-remove-colour = Remove Colour
pin-palette-code = Code
pin-code-hex = HEX
pin-code-rgb = RGB
pin-code-hsl = HSL

# The chips a pin wears.
pin-badge-hdr = HDR
pin-badge-clipboard = Clipboard

# The command line.
cli = CrabSnap: capture the screen, and keep a piece of it on top
cli-lang = Interface language for this run: auto or a language tag
cli-no_daemon = Never use the daemon: run the command in this process
cli-shot = Capture the screen into a file, or onto the screen
cli-shot-file = Write the capture here; the screenshot directory when omitted
cli-shot-format = Write this format (png, jpg, webp, avif, jxl, tiff, bmp, gif, qoi); the file name, then the config, decide when omitted
cli-pin-export-format = Write this format (png, jpg, webp, avif, jxl, tiff, bmp, gif, qoi); the file name decides when omitted
cli-shot-pin = Put the capture on the screen instead of writing it
cli-shot-delay = Wait this many seconds before capturing
cli-shot-output = Capture one output, with no overlay
cli-shot-output-clipboard = Put the capture on the clipboard; the daemon keeps it after this exits
cli-shot-output-output = Capture this output; the first one when omitted
cli-shot-output-backend = Pin one capture backend (ext, screencopy, portal)
cli-shot-region = Freeze the screen and drag a selection
cli-shot-region-demo = Paint a fixed selection instead of dragging one
cli-pin = Operate the pins on the screen
cli-pin-add = Put files on the screen
cli-pin-add-file = The files to pin; each becomes one pin
cli-pin-add-clipboard = Pin what the clipboard holds: a file list pins one per file, an image pins once
cli-pin-add-as = Read the files as this kind instead of detecting it: bitmap, vector, text, markdown, code, latex or palette
cli-pin-list = List the pins the daemon holds
cli-pin-export = Encode one on-screen pin into a file
cli-pin-export-id = Which pin, by the id `pin list` printed
cli-pin-export-file = Write the encoded image here
cli-pin-edit = Open the annotation editor over one pin
cli-pin-edit-id = Which pin, by the id `pin list` printed
cli-pin-close = Close a pin
cli-pin-close-id = Which pin; the one under the pointer when omitted
cli-pin-show = Show every pin again
cli-pin-hide = Hide every pin, keeping them loaded
cli-pin-toggle = Toggle between shown and hidden
cli-info = Read-only answers about the build and the session
cli-info-config = The configuration in use
cli-info-formats = The encodings this build can write
cli-info-formats-verbose = List each encoder's own parameters too
cli-info-status = Capture backend, outputs and the daemon's state
cli-daemon = Run the service: one connection, every surface, the clipboard
cli-daemon-idle = Exit after this many idle seconds; never when omitted
cli-quit = Ask the running daemon to exit
cli-reload = Re-read the configuration and re-probe what it changed
cli-completions = Print a shell completion script
cli-completions-shell = Which shell the script is for
cli-settings = Open the settings window, or write its values without opening one
cli-settings-backend = Write this capture backend into the configuration; auto removes the override
cli-settings-format = Write this save format into the configuration; auto removes the override
cli-settings-directory = Write this shot directory into the configuration; an empty value removes it
cli-settings-ffmpeg = Write this ffmpeg program path into the configuration
cli-settings-ui_language = Write this interface language into the configuration; auto follows the system
cli-settings-shortcut = Write one shortcut as NAME=CHORD; repeat for more, an empty chord clears it
cli-settings-save = Write the values into the configuration and ask the daemon to reload, without a window

# The settings window
settings-title = CrabSnap settings
settings-backend-label = Capture backend
settings-backend-auto = Auto
settings-backend-ext = ext
settings-backend-screencopy = screencopy
settings-backend-portal = portal
settings-backend-wgc = wgc
settings-backend-duplication = duplication
settings-backend-gdi = gdi
settings-format-label = Save format
settings-format-auto = Auto
settings-format-png = PNG
settings-format-jpeg = JPEG
settings-format-webp = WebP
settings-save = Save
settings-close = Close
settings-saved = Saved
settings-file-note = Settings file: { $path }
settings-directory-label = Shot directory
settings-directory-hint = Empty: the pictures directory
settings-ffmpeg-label = ffmpeg
settings-ffmpeg-hint = Empty: found on PATH
settings-lang-label = Language
settings-lang-auto = Automatic
settings-shortcut-hint = Default { $chord }; leave it empty for that, commas separate several
settings-shortcut-cleared = Cleared in the file: this action has no chord
settings-shortcuts-label = Keyboard shortcuts
settings-shortcut-region = Select an area
settings-shortcut-output = Capture an output
settings-shortcut-cancel = Cancel
settings-shortcut-undo = Undo
settings-shortcut-redo = Redo
settings-shortcut-save = Save
settings-shortcut-copy = Copy
settings-shortcut-pin = Pin
settings-shortcut-cursor = Take the cursor tool
settings-shortcut-clear = Deselect the mark, then clear the selection
settings-shortcut-confirm = Confirm the annotation
settings-shortcut-cursor-left = Move the cursor left
settings-shortcut-cursor-right = Move the cursor right
settings-shortcut-cursor-up = Move the cursor up
settings-shortcut-cursor-down = Move the cursor down
settings-bad-chord = { $name }: { $chord } is not a chord
