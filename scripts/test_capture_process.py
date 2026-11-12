"""Save-owned captures through real text/ASCII clients and authenticated transport."""
import base64
import hashlib
import json
import shutil
import struct
import zlib

from process_harness import ProcessTestCase, ROOT, SPECTATOR_TOKEN, read_capture


def png_rgb(data):
    """Independent bounded RGB/RGBA8 PNG oracle, including all five row filters."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise AssertionError("Not a PNG")
    compressed = bytearray()
    offset = 8
    width = height = channels = None
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        payload = data[offset + 8:offset + 8 + size]
        crc = struct.unpack_from(">I", data, offset + 8 + size)[0]
        if zlib.crc32(kind + payload) & 0xffffffff != crc:
            raise AssertionError("PNG checksum mismatch")
        offset += size + 12
        if kind == b"IHDR":
            width, height, depth, color, compression, filtering, interlace = struct.unpack(">IIBBBBB", payload)
            if not (0 < width <= 4096 and 0 < height <= 4096 and depth == 8
                    and color in (2, 6) and (compression, filtering, interlace) == (0, 0, 0)):
                raise AssertionError("Unexpected screenshot PNG format")
            channels = 3 if color == 2 else 4
        elif kind == b"IDAT":
            compressed.extend(payload)
        elif kind == b"IEND":
            break
    if channels is None or offset != len(data):
        raise AssertionError("Incomplete or trailing PNG data")
    stride = width * channels
    decoder = zlib.decompressobj()
    raw = decoder.decompress(compressed, (stride + 1) * height + 1)
    if len(raw) != (stride + 1) * height or not decoder.eof or decoder.unused_data:
        raise AssertionError("Wrong PNG pixel payload size")
    previous = bytearray(stride)
    rgb = bytearray()
    for y in range(height):
        start = y * (stride + 1)
        mode = raw[start]
        row = bytearray(raw[start + 1:start + 1 + stride])
        for x, value in enumerate(row):
            left = row[x - channels] if x >= channels else 0
            above = previous[x]
            corner = previous[x - channels] if x >= channels else 0
            if mode == 0:
                predictor = 0
            elif mode == 1:
                predictor = left
            elif mode == 2:
                predictor = above
            elif mode == 3:
                predictor = (left + above) // 2
            elif mode == 4:
                p = left + above - corner
                distances = [abs(p - left), abs(p - above), abs(p - corner)]
                predictor = [left, above, corner][distances.index(min(distances))]
            else:
                raise AssertionError("Unknown PNG row filter")
            row[x] = (value + predictor) & 255
        for x in range(0, stride, channels):
            rgb.extend(row[x:x + 3])
        previous = row
    return width, height, bytes(rgb)


class CaptureEvidence:
    """Shared assertions only: discovery belongs to the concrete process classes."""

    def start_capture_game(self):
        self.package = self.directory / "capture-source"
        shutil.copytree(ROOT / "scenarios/two-room", self.package)
        self.game = self.server(scenario=self.package)

    def annotations(self, reader):
        frame = self.request(reader, {"type": "snapshot"})
        self.assertIsNone(frame["error"])
        return [e for e in frame["history"] if e["content"]["type"] == "annotation"]

    def attachment(self, reader, entry):
        metadata, = entry["content"]["attachments"]
        self.assertEqual(entry["audience"], "private")
        self.assertEqual(entry["author"]["type"], "frontend")
        self.assertEqual(entry["content"]["category"], "note")
        payload = bytearray()
        while len(payload) < metadata["byte_len"]:
            reader.write(json.dumps({"type": "request", "request": {
                "type": "read_attachment", "entry": entry["id"], "index": 0,
                "offset": len(payload)}}))
            frame = self.frame(reader, lambda f: (f.get("message") or {}).get("type") == "attachment_data")
            reply = frame["message"]
            self.assertEqual(reply["entry"], entry["id"])
            self.assertEqual(reply["index"], 0)
            self.assertEqual(reply["offset"], len(payload))
            self.assertEqual(reply["metadata"], metadata)
            chunk = base64.b64decode(reply["data"], validate=True)
            self.assertTrue(0 < len(chunk) <= 8192)
            payload.extend(chunk)
            ready = self.frame(reader, lambda f: f["type"] == "ready")
            self.assertIsNone(ready["error"])
        self.assertEqual(len(payload), metadata["byte_len"])
        self.assertEqual(hashlib.sha256(payload).hexdigest(), metadata["sha256"])
        return bytes(payload)

    def restart_without_sources(self, clients, expected):
        self.flush_save()
        for client in clients:
            client.stop()
        self.game.stop()
        shutil.rmtree(self.package)
        self.assertFalse(self.package.exists())
        self.game = self.server()
        reader, _ = self.client(observe=True)
        entries = self.annotations(reader)
        self.assertEqual(entries, expected)
        return reader


class TextCaptureProcesses(CaptureEvidence, ProcessTestCase):
    def test_exact_stdout_capture_report_and_source_free_restart(self):
        self.start_capture_game()
        text = self.launch("tor-client-text", ["--script", "--connect", self.address], separate_stderr=True)
        text.until(lambda line: line == "Ready.")
        reader, _ = self.client(observe=True)
        text.command("note ordinary  spacing é")
        text.command("report report  spacing 界")
        entries = self.annotations(reader)
        self.assertEqual([e["content"]["text"] for e in entries],
                         ["ordinary  spacing é", "PLAY TEST REPORT\n\nreport  spacing 界"])
        self.assertTrue(all(not e["content"].get("attachments") for e in entries))
        text.command("help")
        # This reads the real emitted stream, retaining blank lines and spacing.
        expected = ("\n".join(text.transcript) + "\n").encode("utf-8")
        self.assertLess(len(expected), 1024 * 1024)
        response = text.command("screenshot")
        self.assertIn("capture accepted", response.lower())
        entries = self.annotations(reader)
        self.assertEqual(len(entries), 3)
        capture = entries[-1]
        self.assertEqual(capture["author"]["component"], "text")
        self.assertEqual(capture["content"]["attachments"][0]["media_type"], "text")
        self.assertEqual(self.attachment(reader, capture), expected)
        # A later explicit capture is distinct, includes the first command's
        # emitted acknowledgement, and cannot mutate the earlier attachment.
        second_expected = ("\n".join(text.transcript) + "\n").encode("utf-8")
        self.assertGreater(len(second_expected), len(expected))
        self.assertIn("capture accepted", text.command(" Screenshot ").lower())
        entries = self.annotations(reader)
        self.assertEqual(len(entries), 4)
        second = entries[-1]
        self.assertNotEqual(second["id"], capture["id"])
        self.assertEqual(self.attachment(reader, second), second_expected)
        self.assertEqual(self.attachment(reader, capture), expected)
        restored = self.restart_without_sources([text, reader], entries)
        self.assertEqual(self.attachment(restored, capture), expected)
        self.assertEqual(self.attachment(restored, second), second_expected)

    def test_spectator_cannot_capture_or_report(self):
        self.start_capture_game()
        spectator, _ = self.text_client(token=SPECTATOR_TOKEN)
        reader, _ = self.client(observe=True)
        before = self.annotations(reader)
        for command in ["screenshot", "report forbidden", "note forbidden"]:
            self.assertIn("read-only", spectator.command(command))
        self.assertEqual(self.annotations(reader), before)


class AsciiCaptureProcesses(CaptureEvidence, ProcessTestCase):
    graphical = True

    def test_presented_game_pixels_capture_and_source_free_restart(self):
        self.start_capture_game()
        diagnostic = self.directory / "presented.ppm"
        window, initial = self.window("--capture", diagnostic)
        reader, _ = self.client(observe=True)
        for key, body in [("note", "ordinary note"), ("play_test_report", "report body")]:
            self.key(window, key)
            window.write(json.dumps({"type": "text", "text": body}))
            self.ascii_frame(window, lambda f: f.get("note") == body)
            self.key(window, "enter")
        entries = self.annotations(reader)
        self.assertEqual([e["content"]["text"] for e in entries],
                         ["ordinary note", "PLAY TEST REPORT\n\nreport body"])
        self.assertTrue(all(not e["content"].get("attachments") for e in entries))
        # Exercise the public extended command route, not an upload fixture.
        self.key(window, "commands")
        window.write(json.dumps({"type": "text", "text": "screensh"}))
        self.ascii_frame(window, lambda f: f.get("command_line") == "screensh")
        completed = self.key(window, "tab")
        self.assertEqual(completed["command_line"], "screenshot")
        self.assertEqual(completed["state"], initial["state"])
        self.assertEqual(self.annotations(reader), entries)
        complete = self.key(window, "enter")
        self.assertIn("Screenshot accepted", complete["status"])
        self.assertEqual(complete["state"], initial["state"])
        magic, dimensions, maximum, pixels = read_capture(diagnostic).split(b"\n", 3)
        self.assertEqual((magic, dimensions, maximum), (b"P6", b"1200 800", b"255"))
        entries = self.annotations(reader)
        self.assertEqual(len(entries), 3)
        capture = entries[-1]
        self.assertEqual(capture["content"]["anchor"], {"type": "state", "snapshot": complete["state"]["snapshot"]})
        self.assertEqual(capture["author"]["component"], "ascii")
        self.assertEqual(capture["content"]["attachments"][0]["media_type"], "png")
        payload = self.attachment(reader, capture)
        width, height, saved_pixels = png_rgb(payload)
        self.assertEqual((width, height), (1200, 800))
        # The message/status strips change upon acknowledgement. The complete
        # real map (all 33 rows) must match the native framebuffer exactly.
        first, last = 92 * width * 3, (92 + 33 * 18) * width * 3
        self.assertEqual(saved_pixels[first:last], pixels[first:last])
        self.assertGreater(len(set(saved_pixels[first:last])), 8)
        restored = self.restart_without_sources([window, reader], entries)
        self.assertEqual(self.attachment(restored, capture), payload)

    def test_spectator_window_cannot_capture(self):
        self.start_capture_game()
        window, _ = self.window(token=SPECTATOR_TOKEN)
        reader, _ = self.client(observe=True)
        before = self.annotations(reader)
        frame = self.key(window, "screenshot")
        self.assertIn("read-only", frame["status"])
        self.assertFalse(frame["busy"])
        self.assertEqual(self.annotations(reader), before)
