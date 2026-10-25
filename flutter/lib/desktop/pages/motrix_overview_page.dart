import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/common/formatter/id_formatter.dart';
import 'package:flutter_hbb/common/motrix_activity.dart';
import 'package:flutter_hbb/consts.dart';
import 'package:flutter_hbb/desktop/pages/desktop_setting_page.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:flutter_hbb/models/server_model.dart';
import 'package:flutter_hbb/models/state_model.dart';
import 'package:flutter_hbb/utils/multi_window_manager.dart';
import 'package:get/get.dart';
import 'package:provider/provider.dart';

/// Motrix-style "Обзор" page: service, access mode, sessions, this desktop,
/// connections and a year of connection activity.
class MotrixOverviewPage extends StatefulWidget {
  /// Help cards (update / install) shown above the cards.
  final Widget? banner;

  const MotrixOverviewPage({Key? key, this.banner}) : super(key: key);

  @override
  State<MotrixOverviewPage> createState() => _MotrixOverviewPageState();
}

class _MotrixOverviewPageState extends State<MotrixOverviewPage> {
  Timer? _timer;
  int _outgoing = 0;
  bool _today = true;
  String _version = '';

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 2), (_) => _refresh());
    _refresh();
    bind.mainGetVersion().then((v) {
      if (mounted) setState(() => _version = v);
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  void _refresh() {
    if (!mounted) return;
    // Open remote desktop / file transfer / ... windows of this client.
    final outgoing = rustDeskWinManager.getActiveWindows().length;
    setState(() => _outgoing = outgoing);
  }

  Color get _text => Theme.of(context).textTheme.titleLarge?.color ?? Colors.white;

  TextStyle get _label => TextStyle(
      fontSize: 11,
      fontWeight: FontWeight.w600,
      letterSpacing: 0.7,
      color: _text.withOpacity(0.78));

  Widget _card({required Widget child, double? height, EdgeInsets? padding}) =>
      Container(
        height: height,
        padding: padding ?? const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: Theme.of(context).cardColor,
          borderRadius: BorderRadius.circular(16),
        ),
        clipBehavior: Clip.antiAlias,
        child: child,
      );

  Widget _cardTitle(String text, {Widget? trailing}) => Row(children: [
        Expanded(
            child: Text(text.toUpperCase(),
                maxLines: 1, overflow: TextOverflow.ellipsis, style: _label)),
        if (trailing != null) trailing,
      ]);

  @override
  Widget build(BuildContext context) {
    final banner = widget.banner;
    return ChangeNotifierProvider.value(
      value: gFFI.serverModel,
      child: Consumer<ServerModel>(builder: (context, model, _) {
        return SingleChildScrollView(
          padding: const EdgeInsets.fromLTRB(16, 4, 12, 16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(motrixText('Overview', 'Обзор'),
                  style: const TextStyle(
                      fontSize: 26, fontWeight: FontWeight.w700)),
              const SizedBox(height: 20),
              if (banner != null) banner,
              Row(children: [
                Expanded(child: _buildService()),
                const SizedBox(width: 16),
                Expanded(child: _buildAccessMode(model)),
                const SizedBox(width: 16),
                Expanded(
                    child: _buildSessions(
                        motrixText('Incoming', 'Входящие'),
                        model.clients.length,
                        const Color(0xFF8B7CF6))),
                const SizedBox(width: 16),
                Expanded(
                    child: _buildSessions(motrixText('Outgoing', 'Исходящие'),
                        _outgoing, const Color(0xFF38BDF8))),
              ]),
              const SizedBox(height: 16),
              Row(children: [
                Expanded(child: _buildDesktop(model)),
                const SizedBox(width: 16),
                Expanded(child: _buildConnections()),
              ]),
              const SizedBox(height: 16),
              _buildActivity(),
            ],
          ),
        );
      }),
    );
  }

  Widget _buildService() {
    final stopped = Get.find<RxBool>(tag: 'stop-service');
    return _card(
      height: 170,
      child: Obx(() {
        final status = stateGlobal.svcStatus.value;
        final String text;
        final Color color;
        if (stopped.value) {
          text = motrixText('Stopped', 'Остановлена');
          color = kColorWarn;
        } else if (status == SvcStatus.ready) {
          text = motrixText('Ready', 'Готова');
          color = const Color(0xFF00BC7D);
        } else if (status == SvcStatus.connecting) {
          text = motrixText('Connecting', 'Подключение…');
          color = kColorWarn;
        } else {
          text = motrixText('No connection', 'Нет связи');
          color = const Color(0xFFF43F5E);
        }
        return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _cardTitle(motrixText('Service', 'Служба'),
              trailing: Icon(Icons.dns_outlined,
                  size: 16, color: _text.withOpacity(0.78))),
          const SizedBox(height: 12),
          Text(text,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style:
                  const TextStyle(fontSize: 24, fontWeight: FontWeight.w700)),
          const Spacer(),
          Row(children: [
            Expanded(
              child: Text(
                '${bind.mainGetAppNameSync()} $_version',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(fontSize: 13, color: _text.withOpacity(0.78)),
              ),
            ),
            Container(
              width: 16,
              height: 16,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                  color: color.withOpacity(0.25), shape: BoxShape.circle),
              child: Container(
                  width: 8,
                  height: 8,
                  decoration:
                      BoxDecoration(color: color, shape: BoxShape.circle)),
            ),
          ]),
        ]);
      }),
    );
  }

  /// Read-only summary of the access settings; a click opens Security.
  Widget _buildAccessMode(ServerModel model) {
    final accessMode = bind.mainGetOptionSync(key: kOptionAccessMode);
    final int current;
    final String name;
    if (model.approveMode == 'click') {
      current = 2;
      name = motrixText('Accept by click', 'С подтверждением');
    } else if (accessMode == 'view') {
      current = 1;
      name = motrixText('View only', 'Только просмотр');
    } else {
      current = 0;
      name = accessMode == 'full'
          ? motrixText('Full access', 'Полный доступ')
          : motrixText('Custom access', 'Особые права');
    }
    const icons = [
      Icons.lock_open_rounded,
      Icons.visibility_outlined,
      Icons.front_hand_outlined
    ];
    void openSecurity() =>
        DesktopSettingPage.switch2page(SettingsTabKey.safety);
    return _card(
      height: 170,
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        _cardTitle(motrixText('Access mode', 'Режим доступа'),
            trailing: InkWell(
              borderRadius: BorderRadius.circular(8),
              onTap: openSecurity,
              child: Icon(Icons.tune_rounded,
                  size: 16, color: _text.withOpacity(0.78)),
            )),
        const SizedBox(height: 14),
        Row(
          children: List.generate(3, (i) {
            final selected = i == current;
            return Padding(
              padding: const EdgeInsets.only(right: 10),
              child: InkWell(
                borderRadius: BorderRadius.circular(12),
                onTap: openSecurity,
                child: Container(
                  width: 40,
                  height: 40,
                  decoration: BoxDecoration(
                    color: selected
                        ? Theme.of(context).hoverColor
                        : Colors.transparent,
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(
                        color: selected
                            ? _text.withOpacity(0.25)
                            : Colors.transparent),
                  ),
                  child: Icon(icons[i],
                      size: 22,
                      color: selected ? _text : _text.withOpacity(0.55)),
                ),
              ),
            );
          }),
        ),
        const Spacer(),
        Text(name,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600)),
      ]),
    );
  }

  Widget _buildSessions(String title, int count, Color line) {
    return _card(
      height: 170,
      padding: EdgeInsets.zero,
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 0),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            _cardTitle(title),
            const SizedBox(height: 12),
            Row(
              crossAxisAlignment: CrossAxisAlignment.baseline,
              textBaseline: TextBaseline.alphabetic,
              children: [
                Text('$count',
                    style: const TextStyle(
                        fontSize: 30, fontWeight: FontWeight.w700)),
                const SizedBox(width: 6),
                Text(motrixText('sessions', 'сеансов'),
                    style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: _text.withOpacity(0.78))),
              ],
            ),
          ]),
        ),
        const Spacer(),
        Container(
          height: 22,
          decoration: BoxDecoration(
            border: Border(top: BorderSide(color: line.withOpacity(0.7), width: 2)),
            gradient: LinearGradient(
              begin: Alignment.topCenter,
              end: Alignment.bottomCenter,
              colors: [line.withOpacity(0.0), line.withOpacity(0.15)],
            ),
          ),
        ),
      ]),
    );
  }

  Widget _stat(String value, String caption) => Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(value,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style:
                  const TextStyle(fontSize: 18, fontWeight: FontWeight.w700)),
          Text(caption.toUpperCase(),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontSize: 11,
                  letterSpacing: 0.5,
                  color: _text.withOpacity(0.6))),
        ],
      );

  Widget _buildDesktop(ServerModel model) {
    final method = model.verificationMethod;
    final methodText = method == kUsePermanentPassword
        ? motrixText('Permanent', 'Постоянный')
        : method == kUseBothPasswords
            ? motrixText('Both', 'Оба')
            : motrixText('One-time', 'Одноразовый');
    final approveText = model.approveMode == 'click'
        ? motrixText('Click', 'Щелчок')
        : model.approveMode == 'password'
            ? translate('Password')
            : motrixText('Both', 'Оба');
    final showOneTime = model.approveMode != 'click' &&
        model.verificationMethod != kUsePermanentPassword;
    final password = showOneTime ? model.serverPasswd.text : '-';
    final divider = Container(
        width: 1,
        height: 34,
        margin: const EdgeInsets.symmetric(horizontal: 14),
        color: _text.withOpacity(0.1));
    return _card(
      height: 170,
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        _cardTitle(translate('Your Desktop'),
            trailing: Tooltip(
              message: translate('Copy to clipboard'),
              child: InkWell(
                borderRadius: BorderRadius.circular(8),
                onTap: () {
                  Clipboard.setData(ClipboardData(
                      text:
                          '${translate('ID')}: ${model.serverId.text}\n${translate('Password')}: $password'));
                  showToast(translate('Copied'));
                },
                child: Icon(Icons.copy_rounded,
                    size: 16, color: _text.withOpacity(0.78)),
              ),
            )),
        const SizedBox(height: 8),
        Text(model.serverId.text,
            maxLines: 1,
            style: const TextStyle(
                fontSize: 30, fontWeight: FontWeight.w700, letterSpacing: 0.6)),
        const Spacer(),
        Row(children: [
          Flexible(child: _stat(password, translate('Password'))),
          divider,
          Flexible(child: _stat(methodText, motrixText('Password type', 'Тип пароля'))),
          divider,
          Flexible(child: _stat(approveText, motrixText('Approval', 'Подтверждение'))),
        ]),
      ]),
    );
  }

  Widget _buildConnections() {
    final days = ConnectionActivity.days();
    final count = _today
        ? (days[ConnectionActivity.dayKey(DateTime.now())] ?? 0)
        : days.values.fold<int>(0, (a, b) => a + b);
    final last = ConnectionActivity.last();
    String lastText = motrixText('No connections yet', 'Подключений пока не было');
    if (last != null && last['id'] is String && last['ts'] is int) {
      final id = last['id'] as String;
      final alias = getPeerAliasForTab(id);
      final ts = DateTime.fromMillisecondsSinceEpoch(last['ts'] as int);
      final time =
          '${ts.hour.toString().padLeft(2, '0')}:${ts.minute.toString().padLeft(2, '0')}';
      final sameDay = ConnectionActivity.dayKey(ts) ==
          ConnectionActivity.dayKey(DateTime.now());
      final when = sameDay
          ? time
          : '${ts.day.toString().padLeft(2, '0')}.${ts.month.toString().padLeft(2, '0')}, $time';
      lastText =
          '${motrixText('Last', 'Последнее')}: ${alias.isEmpty ? formatID(id) : alias}, $when';
    }
    Widget segment(String text, bool selected, VoidCallback onTap) =>
        GestureDetector(
          onTap: onTap,
          child: Container(
            height: 24,
            padding: const EdgeInsets.symmetric(horizontal: 12),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: selected
                  ? (Theme.of(context).brightness == Brightness.dark
                      ? const Color(0xFF0A0A0A)
                      : Colors.white)
                  : Colors.transparent,
              borderRadius: BorderRadius.circular(999),
            ),
            child: Text(text,
                style:
                    const TextStyle(fontSize: 12, fontWeight: FontWeight.w600)),
          ),
        );
    return _card(
      height: 170,
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        _cardTitle(motrixText('Connections', 'Подключения'),
            trailing: Container(
              padding: const EdgeInsets.all(2),
              decoration: BoxDecoration(
                color: Theme.of(context).hoverColor,
                borderRadius: BorderRadius.circular(999),
              ),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                segment(motrixText('Today', 'Сегодня'), _today,
                    () => setState(() => _today = true)),
                segment(motrixText('All time', 'За всё время'), !_today,
                    () => setState(() => _today = false)),
              ]),
            )),
        const SizedBox(height: 6),
        Text('$count',
            style: const TextStyle(fontSize: 30, fontWeight: FontWeight.w700)),
        const Spacer(),
        Text(lastText,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 13, color: _text.withOpacity(0.78))),
      ]),
    );
  }

  Widget _buildActivity() {
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final scale = isDark
        ? const [
            Color(0xFF262626),
            Color(0xFF173A40),
            Color(0xFF1F5F6B),
            Color(0xFF2A8DA0),
            Color(0xFF38BDF8)
          ]
        : const [
            Color(0xFFE2E6E6),
            Color(0xFFB9E3EA),
            Color(0xFF7CC8D6),
            Color(0xFF3AA6BC),
            Color(0xFF16829C)
          ];
    final days = ConnectionActivity.days();
    final maxCount = days.values.fold<int>(0, (a, b) => a > b ? a : b);
    int level(int n) {
      if (n <= 0 || maxCount <= 0) return 0;
      return (1 + (n - 1) * 4 ~/ math.max(maxCount, 1)).clamp(1, 4);
    }

    const cell = 10.0;
    const gap = 3.0;
    const months = [
      'янв.', 'февр.', 'март', 'апр.', 'май', 'июнь',
      'июль', 'авг.', 'сент.', 'окт.', 'нояб.', 'дек.'
    ];
    const monthsEn = [
      'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
      'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'
    ];
    final isRu = motrixText('en', 'ru') == 'ru';
    final dayLabels = isRu
        ? const ['', 'Пн', '', 'Ср', '', 'Пт', '']
        : const ['', 'Mon', '', 'Wed', '', 'Fri', ''];
    final captionStyle = TextStyle(fontSize: 11, color: _text.withOpacity(0.78));

    return _card(
      child: LayoutBuilder(builder: (context, constraints) {
        const labelWidth = 30.0;
        final weeks = math
            .min(52, ((constraints.maxWidth - labelWidth) / (cell + gap)).floor())
            .clamp(1, 52);
        final now = DateTime.now();
        final today = DateTime(now.year, now.month, now.day);
        // Monday of the current week, then go back `weeks - 1` weeks.
        final firstMonday = today
            .subtract(Duration(days: today.weekday - 1))
            .subtract(Duration(days: 7 * (weeks - 1)));
        final monthLabels = <Widget>[];
        int? lastMonth;
        final columns = List.generate(weeks, (w) {
          final weekStart = firstMonday.add(Duration(days: 7 * w));
          if (weekStart.month != lastMonth) {
            if (lastMonth != null || weekStart.day <= 7) {
              monthLabels.add(Positioned(
                left: labelWidth + w * (cell + gap),
                child: Text(
                    (isRu ? months : monthsEn)[weekStart.month - 1],
                    style: captionStyle),
              ));
            }
            lastMonth = weekStart.month;
          }
          return Padding(
            padding: const EdgeInsets.only(right: gap),
            child: Column(
              children: List.generate(7, (d) {
                final day = weekStart.add(Duration(days: d));
                final future = day.isAfter(today);
                final n = days[ConnectionActivity.dayKey(day)] ?? 0;
                return Container(
                  width: cell,
                  height: cell,
                  margin: const EdgeInsets.only(bottom: gap),
                  decoration: BoxDecoration(
                    color: future ? Colors.transparent : scale[level(n)],
                    borderRadius: BorderRadius.circular(2),
                  ),
                );
              }),
            ),
          );
        });
        return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _cardTitle(motrixText('Activity', 'Активность')),
          const SizedBox(height: 12),
          SizedBox(height: 16, child: Stack(children: monthLabels)),
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            SizedBox(
              width: labelWidth,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: dayLabels
                    .map((l) => SizedBox(
                        height: cell + gap,
                        child: Text(l,
                            style: captionStyle.copyWith(
                                fontSize: 10, height: 1.0))))
                    .toList(),
              ),
            ),
            ...columns,
          ]),
          const SizedBox(height: 4),
          Row(mainAxisAlignment: MainAxisAlignment.end, children: [
            Text(motrixText('Less', 'Меньше'), style: captionStyle),
            const SizedBox(width: 6),
            ...scale.map((c) => Container(
                  width: cell,
                  height: cell,
                  margin: const EdgeInsets.only(right: 4),
                  decoration: BoxDecoration(
                      color: c, borderRadius: BorderRadius.circular(2)),
                )),
            const SizedBox(width: 2),
            Text(motrixText('More', 'Больше'), style: captionStyle),
          ]),
        ]);
      }),
    );
  }
}
