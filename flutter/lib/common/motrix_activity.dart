import 'dart:convert';

import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/models/platform_model.dart';

/// Strings of the Motrix-style pages that RustDesk's dictionaries do not
/// have: Russian UI gets Russian text, other languages the English one.
String motrixText(String en, String ru) =>
    translate('Settings') == 'Настройки' ? ru : en;

/// Local log of outgoing connections from the main window, for the overview
/// page: connections per day (last ~year) and the last connected peer.
class ConnectionActivity {
  static const _kOption = 'motrix-connection-activity';
  static const _kKeepDays = 371;

  static String dayKey(DateTime d) =>
      '${d.year.toString().padLeft(4, '0')}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';

  static Map<String, dynamic> _load() {
    try {
      final v = jsonDecode(bind.mainGetLocalOption(key: _kOption));
      if (v is Map<String, dynamic>) return v;
    } catch (_) {}
    return {};
  }

  /// Connections per day, `yyyy-mm-dd` -> count.
  static Map<String, int> days() {
    final raw = _load()['days'];
    if (raw is! Map) return {};
    return raw.map((k, v) => MapEntry(k.toString(), v is int ? v : 0));
  }

  /// Last connection: `{id, ts}` (ts in milliseconds), or null.
  static Map<String, dynamic>? last() {
    final v = _load()['last'];
    return v is Map<String, dynamic> ? v : null;
  }

  static Future<void> record(String id) async {
    final now = DateTime.now();
    final data = _load();
    final days = Map<String, dynamic>.from(data['days'] is Map ? data['days'] : {});
    final today = dayKey(now);
    days[today] = ((days[today] is int) ? days[today] as int : 0) + 1;
    final oldest = dayKey(now.subtract(const Duration(days: _kKeepDays)));
    days.removeWhere((k, _) => k.compareTo(oldest) < 0);
    data['days'] = days;
    data['last'] = {'id': id, 'ts': now.millisecondsSinceEpoch};
    await bind.mainSetLocalOption(key: _kOption, value: jsonEncode(data));
  }
}
