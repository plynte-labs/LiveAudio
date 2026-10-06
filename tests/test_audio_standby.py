# SPDX-License-Identifier: MIT
"""Pruebas unitarias para el centinela de Standby de audio y resiliencia de drivers."""

import sys
import unittest
from unittest.mock import MagicMock, patch

from liveaudio.core.devices import get_input_device_count


class TestGetInputDeviceCount(unittest.TestCase):
    """Verifica que la sonda pasiva de dispositivos reporte la cantidad correcta sin sobrecarga."""

    @patch("sys.platform", "win32")
    def test_windows_uses_wave_in_get_num_devs(self):
        """En Windows debe consultar winmm.waveInGetNumDevs mediante ctypes."""
        mock_ctypes = MagicMock()
        mock_ctypes.windll.winmm.waveInGetNumDevs.return_value = 3
        with patch.dict("sys.modules", {"ctypes": mock_ctypes}):
            count = get_input_device_count()
            self.assertEqual(count, 3)

    @patch("sys.platform", "win32")
    def test_windows_zero_devices(self):
        """En Windows debe retornar 0 si no hay dispositivos conectados."""
        mock_ctypes = MagicMock()
        mock_ctypes.windll.winmm.waveInGetNumDevs.return_value = 0
        with patch.dict("sys.modules", {"ctypes": mock_ctypes}):
            count = get_input_device_count()
            self.assertEqual(count, 0)

    @patch("sys.platform", "linux")
    def test_fallback_queries_sounddevice(self):
        """En plataformas no-Windows debe consultar sounddevice.query_devices sin destruir drivers."""
        fake_devices = [
            {"name": "Mic 1", "max_input_channels": 1, "max_output_channels": 0},
            {"name": "Speaker", "max_input_channels": 0, "max_output_channels": 2},
            {"name": "Mic 2", "max_input_channels": 2, "max_output_channels": 0},
        ]
        with patch("sounddevice.query_devices", return_value=fake_devices):
            count = get_input_device_count()
            self.assertEqual(count, 2)

    @patch("sys.platform", "linux")
    def test_fallback_zero_devices_on_exception(self):
        """Si sounddevice falla o no hay dispositivos, debe retornar 0 de forma segura."""
        with patch("sounddevice.query_devices", side_effect=Exception("PortAudio down")):
            count = get_input_device_count()
            self.assertEqual(count, 0)


class TestAudioStandbySentinel(unittest.TestCase):
    """Verifica que audio_producer maneje Standby sin invocar InputStream ni destruir drivers."""

    def setUp(self):
        self.audio_queue = MagicMock()
        self.log_queue = MagicMock()

    @patch("liveaudio.core.audio.torch.hub.load")
    @patch("liveaudio.core.audio.VadProvisionHeartbeat")
    @patch("liveaudio.core.audio.get_input_device_count")
    @patch("liveaudio.core.audio.sd")
    def test_standby_emitted_when_zero_devices(
        self, mock_sd, mock_get_count, mock_heartbeat, mock_torch_load
    ):
        """Si get_input_device_count retorna 0, debe emitir estado standby y NO abrir InputStream."""
        import threading
        from liveaudio.core.audio import audio_producer

        mock_torch_load.return_value = (MagicMock(), MagicMock())
        mock_get_count.return_value = 0
        stop_event = threading.Event()

        # Al emitir el status de standby, detenemos el productor para que el test concluya
        def log_put(item):
            if isinstance(item, dict) and item.get("key") == "audio" and item.get("state") == "standby":
                stop_event.set()

        self.log_queue.put_nowait.side_effect = log_put

        audio_producer(
            audio_queue=self.audio_queue,
            config={"silence_timeout": 0.5, "max_chunk_duration": 2.0},
            log_queue=self.log_queue,
            stop_event=stop_event,
        )

        mock_sd.InputStream.assert_not_called()
        mock_sd._terminate.assert_not_called()
        mock_sd._initialize.assert_not_called()

    @patch("liveaudio.core.audio.torch.hub.load")
    @patch("liveaudio.core.audio.VadProvisionHeartbeat")
    @patch("liveaudio.core.audio.get_input_device_count")
    @patch("liveaudio.core.audio.sd")
    def test_hotplug_transition_from_standby_to_active(
        self, mock_sd, mock_get_count, mock_heartbeat, mock_torch_load
    ):
        """Al pasar de 0 a 1 dispositivo, debe salir de Standby y abrir InputStream."""
        import threading
        from liveaudio.core.audio import audio_producer

        mock_torch_load.return_value = (MagicMock(), MagicMock())
        mock_get_count.side_effect = [0, 1]
        stop_event = threading.Event()

        # Mock InputStream context manager
        stream_instance = MagicMock()
        stream_instance.active = True
        mock_sd.InputStream.return_value.__enter__.return_value = stream_instance

        # Cuando el stream se abre y reporta escuchando, paramos
        def log_put(item):
            if isinstance(item, dict) and item.get("key") == "audio" and item.get("text") == "Audio: escuchando":
                stop_event.set()

        self.log_queue.put_nowait.side_effect = log_put

        audio_producer(
            audio_queue=self.audio_queue,
            config={"silence_timeout": 0.5, "max_chunk_duration": 2.0},
            log_queue=self.log_queue,
            stop_event=stop_event,
        )

        mock_sd.InputStream.assert_called_once()

    @patch("liveaudio.core.audio.torch.hub.load")
    @patch("liveaudio.core.audio.VadProvisionHeartbeat")
    @patch("liveaudio.core.audio.get_input_device_count")
    @patch("liveaudio.core.audio.sd")
    def test_disconnection_enters_standby_without_terminate_loop(
        self, mock_sd, mock_get_count, mock_heartbeat, mock_torch_load
    ):
        """Al ocurrir PortAudioError y no haber dispositivos (0), entra a Standby sin llamar _terminate."""
        import sounddevice as real_sd
        import threading
        from liveaudio.core.audio import audio_producer

        mock_torch_load.return_value = (MagicMock(), MagicMock())
        # Primero 1 dispositivo para intentar abrir, luego 0 indicando desconexión
        mock_get_count.side_effect = [1, 0, 0, 0]
        stop_event = threading.Event()

        # Hacemos que InputStream falle con PortAudioError
        mock_sd.PortAudioError = real_sd.PortAudioError
        mock_sd.InputStream.side_effect = real_sd.PortAudioError("Device disconnected")

        status_states = []
        def log_put(item):
            if isinstance(item, dict) and item.get("key") == "audio":
                status_states.append(item.get("state"))
                if item.get("state") == "standby":
                    stop_event.set()

        self.log_queue.put_nowait.side_effect = log_put

        audio_producer(
            audio_queue=self.audio_queue,
            config={"silence_timeout": 0.5, "max_chunk_duration": 2.0},
            log_queue=self.log_queue,
            stop_event=stop_event,
        )

        self.assertIn("standby", status_states)
        mock_sd._terminate.assert_not_called()

    @patch("liveaudio.core.audio.time.time")
    @patch("liveaudio.core.audio.torch.hub.load")
    @patch("liveaudio.core.audio.VadProvisionHeartbeat")
    @patch("liveaudio.core.audio.get_input_device_count")
    @patch("liveaudio.core.audio.sd")
    def test_watchdog_respects_startup_grace_period(
        self, mock_sd, mock_get_count, mock_heartbeat, mock_torch_load, mock_time
    ):
        """Dentro de la ventana de gracia inicial (stream_age <= 3.0), no debe dispararse el watchdog."""
        import threading
        from liveaudio.core.audio import audio_producer

        mock_torch_load.return_value = (MagicMock(), MagicMock())
        mock_get_count.return_value = 1
        stop_event = threading.Event()

        # Secuencia de tiempos: inicio en 100.0, avance a 102.5 (age=2.5s, elapsed=2.5s)
        # El antiguo watchdog de 2s habría fallado aquí; con la ventana de gracia no falla
        times = [100.0, 100.0, 100.0, 102.5, 102.5, 102.5, 102.5]
        mock_time.side_effect = lambda: times.pop(0) if times else 102.5

        stream_instance = MagicMock()
        stream_instance.active = True
        mock_sd.InputStream.return_value.__enter__.return_value = stream_instance

        # Cuando entre al bucle del stream y haga el primer sleep, paramos limpiamente
        def mock_sleep(ms):
            stop_event.set()

        mock_sd.sleep.side_effect = mock_sleep

        audio_producer(
            audio_queue=self.audio_queue,
            config={"silence_timeout": 0.5, "max_chunk_duration": 2.0},
            log_queue=self.log_queue,
            stop_event=stop_event,
        )

        # No debe haber ocurrido excepción de PortAudio en reconexión
        mock_sd._terminate.assert_not_called()

    @patch("liveaudio.core.audio.time.time")
    @patch("liveaudio.core.audio.torch.hub.load")
    @patch("liveaudio.core.audio.VadProvisionHeartbeat")
    @patch("liveaudio.core.audio.get_input_device_count")
    @patch("liveaudio.core.audio.sd")
    def test_watchdog_timeout_fires_after_grace_and_silence(
        self, mock_sd, mock_get_count, mock_heartbeat, mock_torch_load, mock_time
    ):
        """Tras superar la ventana de gracia (stream_age > 3.0) y > 5.0s sin callbacks, dispara el watchdog."""
        import sounddevice as real_sd
        import threading
        from liveaudio.core.audio import audio_producer

        mock_torch_load.return_value = (MagicMock(), MagicMock())
        mock_get_count.return_value = 1
        stop_event = threading.Event()
        mock_sd.PortAudioError = real_sd.PortAudioError

        # Secuencia de tiempos: inicio en 100.0, luego salto a 106.0 (age=6.0s > 3.0s, elapsed=6.0s > 5.0s)
        times = [100.0, 100.0, 100.0, 106.0, 106.0, 106.0, 106.0, 106.0]
        mock_time.side_effect = lambda: times.pop(0) if times else 106.0

        stream_instance = MagicMock()
        stream_instance.active = True
        mock_sd.InputStream.return_value.__enter__.return_value = stream_instance

        reconnected = threading.Event()
        def log_put(item):
            if isinstance(item, dict) and item.get("key") == "audio" and item.get("state") == "warn":
                reconnected.set()
                stop_event.set()

        self.log_queue.put_nowait.side_effect = log_put

        audio_producer(
            audio_queue=self.audio_queue,
            config={"silence_timeout": 0.5, "max_chunk_duration": 2.0},
            log_queue=self.log_queue,
            stop_event=stop_event,
        )

        self.assertTrue(reconnected.is_set(), "Watchdog should have triggered reconnection warning on elapsed > 5s")


if __name__ == "__main__":
    unittest.main()
