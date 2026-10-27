"""Native graphics evidence parser regressions; no GPU or package required."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('linux_package_qa',
                                             Path(__file__).with_name('test_linux_package.py'))
QA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QA)


class GraphicsLogTests(unittest.TestCase):
    def test_actual_ubuntu_stdout_with_nested_adapter_parentheses(self):
        # Exact diagnostic lines from the Ubuntu 24.04 candidate artifact.
        stdout = ('Linux window system: X11/XWayland\n'
                  'Graphics: llvmpipe (LLVM 20.1.2, 256 bits) (Gl, Cpu)\n')
        self.assertEqual(QA.graphics_adapter(stdout, ''),
                         {'name': 'llvmpipe (LLVM 20.1.2, 256 bits)',
                          'backend': 'Gl', 'device_type': 'Cpu'})

    def test_gl_other_and_vulkan_cpu_are_accepted_on_either_stream(self):
        for backend, device_type in [('Gl', 'Other'), ('Vulkan', 'Cpu')]:
            line = f'Graphics: llvmpipe (LLVM 20.1.2, 256 bits) ({backend}, {device_type})\n'
            for stdout, stderr in [(line, ''), ('', line), ('unrelated stdout\n', line)]:
                with self.subTest(backend=backend, stdout=bool(stdout), stderr=bool(stderr)):
                    value = QA.graphics_adapter(stdout, stderr)
                    self.assertEqual(value['name'], 'llvmpipe (LLVM 20.1.2, 256 bits)')
                    self.assertEqual(value['backend'], backend)
                    self.assertEqual(value['device_type'], device_type)
        self.assertEqual(QA.graphics_adapter('', 'Graphics: NV136 (Gl, Other)\r\n')['name'], 'NV136')

    def test_exact_fallback_notice_is_distinct_from_actual_adapter_record(self):
        actual = 'Graphics: llvmpipe (LLVM 20.1.2, 256 bits) (Vulkan, Cpu)\n'
        notice = ('Graphics: no matching OpenGL adapter supports this window surface; '
                  'using the compatible Vulkan adapter\n')
        self.assertEqual(QA.graphics_adapter(actual, notice)['backend'], 'Vulkan')
        self.assertEqual(QA.graphics_adapter(notice, actual)['device_type'], 'Cpu')
        for stdout, stderr in [('', notice), (actual + actual, notice),
                               (actual, 'Graphics: unknown informational message\n')]:
            with self.subTest(stdout=stdout, stderr=stderr), self.assertRaises(RuntimeError):
                QA.graphics_adapter(stdout, stderr)

    def test_missing_or_duplicate_initializations_are_rejected(self):
        line = 'Graphics: NV136 (Gl, Other)\n'
        for stdout, stderr in [('', ''), ('Linux window system: X11/XWayland\n', ''),
                               (line + line, ''), ('', line + line), (line, line),
                               (line + 'Graphics: malformed\n', '')]:
            with self.subTest(stdout=stdout, stderr=stderr), self.assertRaises(RuntimeError):
                QA.graphics_adapter(stdout, stderr)

    def test_malformed_multiline_names_and_device_types_are_rejected(self):
        for line in [
                'Graphics: llvmpipe\n (Gl, Other)\n',
                'Graphics: llvmpipe\r (Gl, Other)\n',
                'Graphics: llvmpipe (LLVM\n20.1.2, 256 bits) (Gl, Other)\n',
                'Graphics: llvmpipe (Gl, Oth\ner)\n',
                'Graphics: llvmpipe (Vulkan, C\rpu)\n',
                'Graphics: llvmpipe (Gl, Unknown)\n',
                'Graphics: llvmpipe (Metal, Other)\n',
                'Graphics:    (Gl, Other)\n',
                'Graphics: llvmpipe (Gl, Other) extra trailing data\n']:
            for stdout, stderr in [(line, ''), ('', line)]:
                with self.subTest(line=line), self.assertRaises(RuntimeError):
                    QA.graphics_adapter(stdout, stderr)

    def test_pinned_graphics_errors_are_accepted_but_unrelated_failures_are_not(self):
        for error in [
                'hejjetrac: WGPU error: No suitable graphics adapter found; gl found no adapters',
                'hejjetrac: WGPU error: Adapter selection failed: no compatible adapter',
                'hejjetrac: WGPU error: Failed to create surface for any enabled backend: {Gl: InstanceError}']:
            self.assertEqual(QA.graphics_failure(error), error)
        for unrelated in [
                'error while loading shared libraries: libEGL.so',
                'xvfb-run: error: Xvfb failed to start',
                'hejjetrac: cannot decode source',
                'hejjetrac: source named no suitable graphics adapter found is missing',
                'hejjetrac: WGPU error: No suitable graphics adapter found\nthread main panicked']:
            with self.subTest(unrelated=unrelated), self.assertRaises(RuntimeError):
                QA.graphics_failure(unrelated)


if __name__ == '__main__':
    unittest.main()
