import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart';
import 'package:flutter_hbb/common/motrix_activity.dart';
import 'package:flutter_hbb/common/widgets/peer_tab_page.dart';
import 'package:flutter_hbb/consts.dart';
import 'package:flutter_hbb/desktop/pages/connection_page.dart';
import 'package:flutter_hbb/desktop/pages/desktop_tab_page.dart';
import 'package:flutter_hbb/desktop/widgets/tabbar_widget.dart';
import 'package:flutter_hbb/models/peer_tab_model.dart';
import 'package:flutter_hbb/models/platform_model.dart';
import 'package:get/get.dart';
import 'package:provider/provider.dart';
import 'package:window_manager/window_manager.dart';

const double _kExpandedWidth = 210;
const double _kCollapsedWidth = 64;

/// Left sidebar of the Motrix-style main window: app header (logo, collapse,
/// new connection), peer sections and settings. It lives outside the tab
/// view, so it stays visible on every main window tab.
class MotrixSidebar extends StatelessWidget {
  final DesktopTabController tabController;

  MotrixSidebar({Key? key, required this.tabController}) : super(key: key);

  static final RxBool collapsed = false.obs;

  /// The home tab shows the overview page; it is the first page on start.
  static final RxBool overviewSelected = true.obs;

  void _showHome() => tabController.jumpToByKey(kTabLabelHomePage);

  @override
  Widget build(BuildContext context) {
    return Obx(() {
      final isCollapsed = collapsed.value;
      final selectedKey = () {
        final state = tabController.state.value;
        if (state.selected < 0 || state.selected >= state.tabs.length) {
          return '';
        }
        return state.tabs[state.selected].key;
      }();
      return AnimatedContainer(
        duration: const Duration(milliseconds: 160),
        width: isCollapsed ? _kCollapsedWidth : _kExpandedWidth,
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _buildHeader(context, isCollapsed),
            const SizedBox(height: 16),
            _MotrixNavItem(
              icon: Icons.space_dashboard_outlined,
              label: motrixText('Overview', 'Обзор'),
              collapsed: isCollapsed,
              selected:
                  selectedKey == kTabLabelHomePage && overviewSelected.value,
              onTap: () {
                _showHome();
                overviewSelected.value = true;
              },
            ),
            ChangeNotifierProvider.value(
              value: gFFI.peerTabModel,
              child: Consumer<PeerTabModel>(
                builder: (context, model, _) => Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: model.visibleEnabledOrderedIndexs
                      .map((t) => _MotrixNavItem(
                            icon: model.tabIcon(t),
                            label: model.tabTooltip(t),
                            collapsed: isCollapsed,
                            selected: selectedKey == kTabLabelHomePage &&
                                !overviewSelected.value &&
                                model.currentTab == t,
                            onTap: isOptionFixed(kOptionPeerTabIndex)
                                ? null
                                : () {
                                    _showHome();
                                    overviewSelected.value = false;
                                    selectPeerTab(t);
                                  },
                          ))
                      .toList(),
                ),
              ),
            ),
            const Spacer(),
            if (!bind.isDisableSettings()) ...[
              Divider(
                  height: 9,
                  thickness: 1,
                  color: _foreground(context).withOpacity(0.10)),
              _MotrixNavItem(
                icon: Icons.settings_outlined,
                label: translate('Settings'),
                collapsed: isCollapsed,
                selected: selectedKey == kTabLabelSettingPage,
                onTap: () => DesktopTabPage.onAddSetting(),
              ),
            ],
          ],
        ),
      );
    });
  }

  Widget _buildHeader(BuildContext context, bool isCollapsed) {
    final fg = _foreground(context);
    final toggle = _HeaderButton(
      icon: isCollapsed ? Icons.view_sidebar_outlined : Icons.vertical_split_outlined,
      tooltip: translate(isCollapsed ? 'Show Toolbar' : 'Hide Toolbar'),
      onTap: () => collapsed.value = !collapsed.value,
    );
    final newConnection = _HeaderButton(
      icon: Icons.add,
      tooltip: translate('New Connection'),
      onTap: () {
        _showHome();
        overviewSelected.value = false;
        // Let the connection page become visible before focusing the field.
        WidgetsBinding.instance.addPostFrameCallback(
            (_) => ConnectionPage.requestIdFocus?.call());
      },
    );
    // The header doubles as the window drag area (the title bar is gone).
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onPanStart: (_) => windowManager.startDragging(),
      onDoubleTap: () => toggleMaximize(true),
      child: isCollapsed
          // Collapsed: app icon on top, the buttons below it.
          ? Column(mainAxisSize: MainAxisSize.min, children: [
              SizedBox(height: 36, child: Center(child: loadIcon(24))),
              const SizedBox(height: 6),
              toggle,
              const SizedBox(height: 2),
              newConnection,
            ])
          : SizedBox(
        height: 36,
        child: Row(
                children: [
                  loadIcon(18).marginOnly(left: 4, right: 8),
                  Expanded(
                    child: Text(
                      bind.mainGetAppNameSync(),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                          color: fg),
                    ),
                  ),
                  toggle,
                  newConnection,
                ],
              ),
      ),
    );
  }
}

Color _foreground(BuildContext context) =>
    Theme.of(context).brightness == Brightness.dark
        ? const Color(0xFFF2F4F4)
        : const Color(0xFF191B22);

class _HeaderButton extends StatelessWidget {
  final IconData icon;
  final String tooltip;
  final VoidCallback onTap;

  const _HeaderButton(
      {required this.icon, required this.tooltip, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: tooltip,
      child: InkWell(
        borderRadius: BorderRadius.circular(8),
        hoverColor: _foreground(context).withOpacity(0.10),
        onTap: onTap,
        child: SizedBox(
          width: 28,
          height: 28,
          child: Icon(icon,
              size: 17, color: _foreground(context).withOpacity(0.8)),
        ),
      ),
    );
  }
}

/// Motrix nav item: 40px, radius 10, translucent white fill when selected.
class _MotrixNavItem extends StatefulWidget {
  final IconData icon;
  final String label;
  final bool selected;
  final bool collapsed;
  final VoidCallback? onTap;

  const _MotrixNavItem({
    required this.icon,
    required this.label,
    required this.selected,
    required this.collapsed,
    required this.onTap,
  });

  @override
  State<_MotrixNavItem> createState() => _MotrixNavItemState();
}

class _MotrixNavItemState extends State<_MotrixNavItem> {
  bool _hover = false;

  @override
  Widget build(BuildContext context) {
    final fg = _foreground(context);
    final background = widget.selected
        ? fg.withOpacity(0.14)
        : (_hover ? fg.withOpacity(0.08) : Colors.transparent);
    final item = Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: MouseRegion(
        onEnter: (_) => setState(() => _hover = true),
        onExit: (_) => setState(() => _hover = false),
        child: GestureDetector(
          onTap: widget.onTap,
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 160),
            height: 40,
            padding: EdgeInsets.symmetric(horizontal: widget.collapsed ? 0 : 12),
            decoration: BoxDecoration(
              color: background,
              borderRadius: BorderRadius.circular(10),
            ),
            child: Row(
              mainAxisAlignment: widget.collapsed
                  ? MainAxisAlignment.center
                  : MainAxisAlignment.start,
              children: [
                Icon(widget.icon, size: 18, color: fg),
                if (!widget.collapsed) ...[
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      widget.label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 15, fontWeight: FontWeight.w500, color: fg),
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
    return widget.collapsed
        ? Tooltip(message: widget.label, child: item)
        : item;
  }
}
