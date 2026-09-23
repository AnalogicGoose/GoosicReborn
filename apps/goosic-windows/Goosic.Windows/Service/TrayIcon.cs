using System;
using System.Runtime.InteropServices;

namespace Goosic.Windows.Service;

/// <summary>
/// Goosic's icon in the notification area, with a menu to reopen, play or pause, skip, or quit.
/// </summary>
/// <remarks>
/// Written against Shell_NotifyIcon directly: the Windows App SDK has no notification-area API,
/// and this is one hidden message window and one menu, not enough to justify a dependency. The
/// message window is created on the UI thread, whose loop pumps it, so every event is raised on
/// the UI thread.
/// </remarks>
internal sealed class TrayIcon : IDisposable
{
    private const int WM_APP_TRAY = 0x8001;
    private const int WM_LBUTTONUP = 0x0202;
    private const int WM_RBUTTONUP = 0x0205;
    private const int WM_CONTEXTMENU = 0x007B;
    private const int WM_NULL = 0x0000;
    private const uint NIM_ADD = 0, NIM_MODIFY = 1, NIM_DELETE = 2;
    private const uint NIF_MESSAGE = 1, NIF_ICON = 2, NIF_TIP = 4;
    private const uint MF_STRING = 0, MF_SEPARATOR = 0x800;
    private const uint TPM_RETURNCMD = 0x100, TPM_RIGHTBUTTON = 2, TPM_BOTTOMALIGN = 0x20;
    private const int IDI_APPLICATION = 32512;
    private static readonly IntPtr HWND_MESSAGE = new(-3);

    private enum Command { Open = 1, PlayPause, Next, Quit }

    private readonly WndProc _procedure;
    private readonly string _className = "GoosicTray" + Environment.ProcessId;
    private readonly uint _taskbarCreated;
    private readonly IntPtr _window;
    private string _tooltip = "Goosic";
    private bool _disposed;

    internal event Action? Opened;
    internal event Action? PlayPauseRequested;
    internal event Action? NextRequested;
    internal event Action? QuitRequested;

    internal TrayIcon(string? iconPath)
    {
        _procedure = Procedure;
        var instance = GetModuleHandle(null);
        var windowClass = new WNDCLASSEX
        {
            cbSize = (uint)Marshal.SizeOf<WNDCLASSEX>(),
            lpfnWndProc = Marshal.GetFunctionPointerForDelegate(_procedure),
            hInstance = instance,
            lpszClassName = _className,
        };
        if (RegisterClassEx(ref windowClass) == 0)
        {
            throw new InvalidOperationException($"Tray window class failed ({Marshal.GetLastWin32Error()}).");
        }

        _window = CreateWindowEx(0, _className, "", 0, 0, 0, 0, 0, HWND_MESSAGE, IntPtr.Zero, instance, IntPtr.Zero);
        if (_window == IntPtr.Zero)
        {
            throw new InvalidOperationException($"Tray window failed ({Marshal.GetLastWin32Error()}).");
        }

        // Explorer restarting takes every icon with it; this is how it says to add them back.
        _taskbarCreated = RegisterWindowMessage("TaskbarCreated");
        Icon = iconPath is not null ? ExtractIcon(instance, iconPath, 0) : IntPtr.Zero;
        if (Icon == IntPtr.Zero || Icon == new IntPtr(1))
        {
            Icon = LoadIcon(IntPtr.Zero, new IntPtr(IDI_APPLICATION));
        }

        Notify(NIM_ADD);
    }

    private IntPtr Icon { get; }

    /// <summary>What the icon says on hover: the song that is playing, or just Goosic.</summary>
    internal void SetTooltip(string text)
    {
        // The notification area truncates at 127 characters and refuses longer text outright.
        _tooltip = text.Length > 127 ? text[..126] + "…" : text;
        Notify(NIM_MODIFY);
    }

    private void Notify(uint message)
    {
        var data = new NOTIFYICONDATA
        {
            cbSize = (uint)Marshal.SizeOf<NOTIFYICONDATA>(),
            hWnd = _window,
            uID = 1,
            uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage = WM_APP_TRAY,
            hIcon = Icon,
            szTip = _tooltip,
        };
        Shell_NotifyIcon(message, ref data);
    }

    private IntPtr Procedure(IntPtr window, uint message, IntPtr wParam, IntPtr lParam)
    {
        if (message == WM_APP_TRAY)
        {
            switch ((int)lParam & 0xFFFF)
            {
                case WM_LBUTTONUP: Opened?.Invoke(); break;
                case WM_RBUTTONUP or WM_CONTEXTMENU: ShowMenu(); break;
            }

            return IntPtr.Zero;
        }

        if (message == _taskbarCreated && _taskbarCreated != 0)
        {
            Notify(NIM_ADD);
        }

        return DefWindowProc(window, message, wParam, lParam);
    }

    private void ShowMenu()
    {
        var menu = CreatePopupMenu();
        AppendMenu(menu, MF_STRING, (UIntPtr)Command.Open, "Open Goosic");
        AppendMenu(menu, MF_SEPARATOR, UIntPtr.Zero, null);
        AppendMenu(menu, MF_STRING, (UIntPtr)Command.PlayPause, "Play / Pause");
        AppendMenu(menu, MF_STRING, (UIntPtr)Command.Next, "Next");
        AppendMenu(menu, MF_SEPARATOR, UIntPtr.Zero, null);
        AppendMenu(menu, MF_STRING, (UIntPtr)Command.Quit, "Quit Goosic");
        // Without the foreground call the menu will not close when the user clicks elsewhere.
        SetForegroundWindow(_window);
        GetCursorPos(out var point);
        var chosen = (Command)TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            point.X, point.Y, 0, _window, IntPtr.Zero);
        PostMessage(_window, WM_NULL, IntPtr.Zero, IntPtr.Zero);
        DestroyMenu(menu);
        switch (chosen)
        {
            case Command.Open: Opened?.Invoke(); break;
            case Command.PlayPause: PlayPauseRequested?.Invoke(); break;
            case Command.Next: NextRequested?.Invoke(); break;
            case Command.Quit: QuitRequested?.Invoke(); break;
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        Notify(NIM_DELETE);
        DestroyWindow(_window);
        UnregisterClass(_className, GetModuleHandle(null));
    }

    private delegate IntPtr WndProc(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WNDCLASSEX
    {
        public uint cbSize, style;
        public IntPtr lpfnWndProc;
        public int cbClsExtra, cbWndExtra;
        public IntPtr hInstance, hIcon, hCursor, hbrBackground;
        public string? lpszMenuName, lpszClassName;
        public IntPtr hIconSm;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NOTIFYICONDATA
    {
        public uint cbSize;
        public IntPtr hWnd;
        public uint uID, uFlags, uCallbackMessage;
        public IntPtr hIcon;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string szTip;
        public uint dwState, dwStateMask;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string szInfo;
        public uint uVersion;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string szInfoTitle;
        public uint dwInfoFlags;
        public Guid guidItem;
        public IntPtr hBalloonIcon;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct POINT { public int X, Y; }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)] private static extern bool Shell_NotifyIcon(uint message, ref NOTIFYICONDATA data);
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr ExtractIcon(IntPtr instance, string path, int index);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)] private static extern ushort RegisterClassEx(ref WNDCLASSEX windowClass);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool UnregisterClass(string className, IntPtr instance);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateWindowEx(uint exStyle, string className, string name, uint style, int x, int y,
        int width, int height, IntPtr parent, IntPtr menu, IntPtr instance, IntPtr param);
    [DllImport("user32.dll")] private static extern bool DestroyWindow(IntPtr window);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr DefWindowProc(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern uint RegisterWindowMessage(string name);
    [DllImport("user32.dll")] private static extern IntPtr LoadIcon(IntPtr instance, IntPtr name);
    [DllImport("user32.dll")] private static extern IntPtr CreatePopupMenu();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] private static extern bool AppendMenu(IntPtr menu, uint flags, UIntPtr id, string? text);
    [DllImport("user32.dll")] private static extern bool DestroyMenu(IntPtr menu);
    [DllImport("user32.dll")] private static extern int TrackPopupMenu(IntPtr menu, uint flags, int x, int y, int reserved, IntPtr window, IntPtr rect);
    [DllImport("user32.dll")] private static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool GetCursorPos(out POINT point);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] private static extern IntPtr GetModuleHandle(string? name);
}
