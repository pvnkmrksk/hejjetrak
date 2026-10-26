// Test tooling only. These externally supplied historical assemblies are never
// an application dependency or included in a HejjéTrac binary package.
using System.Diagnostics;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text.Json;
using NetMQ;
using NetMQ.Sockets;

AsyncIO.ForceDotNet.Force();
Probe.Run(args);

static class Probe
{
    static void Emit(object value)
    {
        Console.WriteLine(JsonSerializer.Serialize(value));
        Console.Out.Flush();
    }

    // Keep NetMQ type initialization after ForceDotNet on non-Windows hosts.
    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void Run(string[] args)
    {
        if (args.Length != 2) throw new ArgumentException("ENDPOINT SECONDS required");
        var version = typeof(SubscriberSocket).Assembly.GetName();
        if (version.Version != new Version(4, 0, 0, 1))
            throw new InvalidOperationException("This probe requires unchanged NetMQ 4.0.0.1");
        Emit(new { kind = "assembly", name = version.FullName });
        using (var socket = new SubscriberSocket())
        {
            socket.Options.Linger = TimeSpan.Zero;
            socket.Connect(args[0]);
            socket.Subscribe("");
            Emit(new { kind = "ready" });
            var duration = double.Parse(args[1], System.Globalization.CultureInfo.InvariantCulture);
            var watch = Stopwatch.StartNew();
            var count = 0;
            while (watch.Elapsed.TotalSeconds < duration)
            {
                var frames = new List<string>();
                if (socket.TryReceiveMultipartStrings(TimeSpan.FromMilliseconds(100), ref frames))
                {
                    ++count;
                    Emit(new { kind = "message", frames });
                }
            }
            Emit(new { kind = "complete", count });
        }
        NetMQConfig.Cleanup(false);
        Emit(new { kind = "cleanup_complete" });
    }
}
