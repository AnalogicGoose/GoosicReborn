using System;
using System.Runtime.InteropServices;
using Goosic.Windows.Service;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.Windows.AppLifecycle;

namespace Goosic.Windows;

public partial class App : Application
{
    private MainWindow? _window;
    private AppInstance? _instance;

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool AllowSetForegroundWindow(uint processId);

    public App()
    {
        InitializeComponent();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        // Register before creating MainWindow: its constructor starts the private Rust service
        // and playback host. A redirected launch must never create a second playback session.
        var instance = AppInstance.FindOrRegisterForKey("io.github.analogicgoose.Goosic");
        if (!instance.IsCurrent)
        {
            try
            {
                // The user-launched process can let the existing hidden window take focus.
                _ = AllowSetForegroundWindow(instance.ProcessId);
                await instance.RedirectActivationToAsync(AppInstance.GetCurrent().GetActivatedEventArgs());
            }
            catch (Exception error)
            {
                BridgeLog.Write($"activation redirection failed: {error.GetType().Name}");
            }
            // Await on the running XAML dispatcher instead of blocking an STA thread.
            Exit();
            return;
        }

        _instance = instance;
        var dispatcher = DispatcherQueue.GetForCurrentThread();
        _instance.Activated += (_, activation) =>
        {
            // Automatic sign-in launches should not pull an already running app forward.
            if (activation.Data is global::Windows.ApplicationModel.Activation.ILaunchActivatedEventArgs launch
                && Array.Exists(launch.Arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries),
                    argument => argument == StartupRegistration.BackgroundArgument))
            {
                return;
            }
            dispatcher.TryEnqueue(() => _window?.ShowForActivation());
        };
        _window = new MainWindow();
        _window.Activate();
    }
}
