Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$wavPath = "$PSScriptRoot\reference_speech.wav"
$synth.SetOutputToWaveFile($wavPath)
$text = "Hello everyone, welcome to the live stream. Today we are testing real time audio transcription."
$synth.Speak($text)
$synth.Dispose()
Write-Host "Generated speech at: $wavPath"
