import 'package:flutter/material.dart';
import 'package:flutter_hbb/common.dart' show gFFI;
import 'package:flutter_hbb/models/platform_model.dart';

const _kDesktopUiFlavor = String.fromEnvironment('DESKTOP_UI_FLAVOR');

String _t(String name) => bind.translate(name: name, locale: localeName);

/// Cashdesk: block stop/quit while incoming sessions are active unless password matches.
Future<bool> confirmCashdeskExitIfSessionsActive() async {
  if (_kDesktopUiFlavor != 'cashdesk') {
    return true;
  }
  final count = bind.mainControlledSessionCount();
  if (count <= 0) {
    return true;
  }

  final password = TextEditingController();
  var err = '';
  final ok = await gFFI.dialogManager.show<bool>((setState, close, context) {
    submit() async {
      final pass = password.text;
      if (pass.isEmpty) {
        setState(() => err = _t('Password'));
        return;
      }
      if (!bind.mainVerifyPermanentPassword(password: pass)) {
        setState(() => err = _t('Wrong password'));
        return;
      }
      close(true);
    }

    return AlertDialog(
      title: Text(_t('Exit password required')),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(_t(
              'Enter password to stop service while remote sessions are active')),
          const SizedBox(height: 12),
          TextField(
            controller: password,
            obscureText: true,
            autofocus: true,
            decoration: InputDecoration(
              labelText: _t('Password'),
              errorText: err.isEmpty ? null : err,
            ),
            onSubmitted: (_) => submit(),
          ),
        ],
      ),
      actions: [
        TextButton(onPressed: close, child: Text(_t('Cancel'))),
        TextButton(onPressed: submit, child: Text(_t('OK'))),
      ],
    );
  });
  return ok == true;
}
