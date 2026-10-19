// S6: prints "<file> | <text>@<x>,<y> ..." for Activity Monitor's bottom panel (the bottom 20% of each screenshot),
// read with Vision; coordinates are relative to that region, so labels pair with the value on the same row.
import AppKit
import Vision

for path in CommandLine.arguments.dropFirst() {
  guard let image = NSImage(contentsOfFile: path),
    let cg = image.cgImage(forProposedRect: nil, context: nil, hints: nil)
  else { continue }
  let request = VNRecognizeTextRequest()
  request.recognitionLevel = .accurate
  request.usesLanguageCorrection = false
  request.regionOfInterest = CGRect(x: 0, y: 0, width: 1, height: 0.2)
  try? VNImageRequestHandler(cgImage: cg).perform([request])
  let found = (request.results ?? []).compactMap { o -> String? in
    guard let text = o.topCandidates(1).first?.string else { return nil }
    return "\(text)@\(String(format: "%.3f,%.3f", o.boundingBox.midX, o.boundingBox.midY))"
  }
  print((path as NSString).lastPathComponent, "|", found.joined(separator: "\t"))
}
