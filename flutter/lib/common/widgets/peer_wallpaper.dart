import 'package:flutter/material.dart';

import '../../common.dart';
import '../../models/platform_model.dart';

/// Peers whose wallpaper the portal does not have; not requested again
/// until the app restarts.
final Set<String> _missingWallpapers = {};

/// Desktop wallpaper thumbnail of a peer, served by the inventory portal
/// (`GET /api/v1/devices/{id}/wallpaper`). Shows [fallback] while loading and
/// when there is no wallpaper.
class PeerWallpaper extends StatelessWidget {
  final String peerId;
  final Widget fallback;

  const PeerWallpaper(
      {Key? key, required this.peerId, required this.fallback})
      : super(key: key);

  @override
  Widget build(BuildContext context) {
    final id = basePeerId(peerId);
    final base = bind.mainGetInventoryPortalBase().trim();
    if (id.isEmpty || base.isEmpty || _missingWallpapers.contains(id)) {
      return fallback;
    }
    // Refresh at most once an hour: the URL is the image cache key.
    final hour = DateTime.now().millisecondsSinceEpoch ~/ 3600000;
    final url =
        '$base/api/v1/devices/${Uri.encodeComponent(id)}/wallpaper?h=$hour';
    final token = bind.mainGetInventoryPortalToken().trim();
    return Image.network(
      url,
      headers: token.isEmpty ? null : {'Authorization': 'Bearer $token'},
      fit: BoxFit.cover,
      gaplessPlayback: true,
      filterQuality: FilterQuality.medium,
      frameBuilder: (context, child, frame, wasSynchronouslyLoaded) {
        if (frame == null) return fallback;
        return Stack(
          fit: StackFit.expand,
          children: [
            child,
            // Keeps the white card text readable on light wallpapers.
            const DecoratedBox(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [Color(0x33000000), Color(0x99000000)],
                ),
              ),
            ),
          ],
        );
      },
      errorBuilder: (context, error, stackTrace) {
        _missingWallpapers.add(id);
        return fallback;
      },
    );
  }
}
