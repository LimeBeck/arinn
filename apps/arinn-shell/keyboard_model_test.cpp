// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

// The on-screen keyboard's layout and focus rules (spec §6.2, ADR-0103,
// handoff KB-1…8).
#include "keyboard_model.h"

#include <QTest>

namespace {
QVariantMap key(const KeyboardModel &model, int row, int column) {
  return model.rows()[row].toList()[column].toMap();
}
QVariantMap focused(const KeyboardModel &model) {
  return key(model, model.row(), model.column());
}
int rowSize(const KeyboardModel &model, int row) {
  return model.rows()[row].toList().size();
}
} // namespace

class KeyboardModelTest : public QObject {
  Q_OBJECT
private slots:
  void init() { KeyboardModel::forgetFields(); }

  void addressFieldHasContextLettersAndActions() {
    KeyboardModel model;
    model.setContext(QStringLiteral("address"));
    model.setSubmitLabel(QStringLiteral("Open"));
    model.open();
    QCOMPARE(model.rows().size(), 5);
    QCOMPARE(key(model, 0, 0).value("label").toString(),
             QStringLiteral("https://"));
    QCOMPARE(key(model, 0, 0).value("width").toReal(), 2.0);
    // Addresses default to English letters, whatever the interface.
    QCOMPARE(model.language(), QStringLiteral("en"));
    QCOMPARE(key(model, 1, 0).value("label").toString(), QStringLiteral("q"));
    for (int row = 1; row <= 3; ++row)
      QCOMPARE(rowSize(model, row), 12);
    QStringList actions;
    for (const auto &entry : model.rows()[4].toList())
      actions << entry.toMap().value("action").toString();
    QCOMPARE(actions, QStringList({"shift", "layer", "language", "insert",
                                   "left", "right", "backspace", "submit"}));
    QCOMPARE(key(model, 4, 3).value("width").toReal(), 3.0);
    QCOMPARE(key(model, 4, 7).value("label").toString(),
             QStringLiteral("Open"));
    QCOMPARE(key(model, 4, 7).value("kind").toString(),
             QStringLiteral("primary"));
  }

  void fieldWithoutContextUsesTheInterfaceLanguage() {
    KeyboardModel model;
    model.setUiLanguage(QStringLiteral("ru"));
    model.open();
    QCOMPARE(model.rows().size(), 4);
    QCOMPARE(model.language(), QStringLiteral("ru"));
    QCOMPARE(key(model, 0, 0).value("label").toString(), QStringLiteral("й"));
    model.setUiLanguage(QStringLiteral("en"));
    model.open();
    QCOMPARE(model.language(), QStringLiteral("en"));
  }

  void pathFieldHasThePathRow() {
    KeyboardModel model;
    model.setContext(QStringLiteral("path"));
    model.open();
    QCOMPARE(model.language(), QStringLiteral("en"));
    QCOMPARE(key(model, 0, 3).value("label").toString(),
             QStringLiteral("/run/media/"));
    QCOMPARE(key(model, 0, 5).value("label").toString(), QStringLiteral(".."));
  }

  void firstOpenFocusesTheFifthKeyOfTheMiddleLetterRow() {
    KeyboardModel model;
    model.setContext(QStringLiteral("address"));
    model.setUiLanguage(QStringLiteral("ru"));
    model.open();
    QCOMPARE(model.row(), 2);
    QCOMPARE(model.column(), 4);
    QCOMPARE(focused(model).value("label").toString(), QStringLiteral("g"));

    KeyboardModel plain;
    plain.setUiLanguage(QStringLiteral("ru"));
    plain.open();
    QCOMPARE(plain.row(), 1);
    QCOMPARE(focused(plain).value("label").toString(), QStringLiteral("п"));
  }

  void leftAndRightDoNotWrap() {
    KeyboardModel model;
    model.open();
    model.focusKey(0, 0);
    QVERIFY(!model.move(QStringLiteral("left")));
    QCOMPARE(model.column(), 0);
    model.focusKey(0, 11);
    QVERIFY(!model.move(QStringLiteral("right")));
    QCOMPARE(model.column(), 11);
    QVERIFY(model.move(QStringLiteral("left")));
    QCOMPARE(model.column(), 10);
  }

  void upAndDownGoToTheNearestKey() {
    KeyboardModel model;
    model.setContext(QStringLiteral("address"));
    model.setSubmitLabel(QStringLiteral("Open"));
    model.open();
    // From the bottom-right letter down to the main key, the rightmost.
    model.focusKey(3, 11);
    QVERIFY(model.move(QStringLiteral("down")));
    QCOMPARE(focused(model).value("action").toString(),
             QStringLiteral("submit"));
    // Space (width 3) from the middle of the action row goes up to a letter
    // above its centre.
    model.focusKey(4, 3);
    QVERIFY(model.move(QStringLiteral("up")));
    QCOMPARE(model.row(), 3);
    QCOMPARE(model.column(), 5);
    // From the first letter row up to https:// (width 2 at the left).
    model.focusKey(1, 1);
    QVERIFY(model.move(QStringLiteral("up")));
    QCOMPARE(model.row(), 0);
    QCOMPARE(model.column(), 0);
    // Down from the bottom row and up from the top row leave focus in place.
    model.focusKey(4, 0);
    QVERIFY(!model.move(QStringLiteral("down")));
    QCOMPARE(model.row(), 4);
    model.focusKey(0, 2);
    QVERIFY(!model.move(QStringLiteral("up")));
    QCOMPARE(model.row(), 0);
  }

  void enteringFromAboveUsesTheNearestTopKey() {
    KeyboardModel model;
    model.open();
    model.enterTop(0.99);
    QCOMPARE(model.row(), 0);
    QCOMPARE(model.column(), 11);
    model.enterTop(0.0);
    QCOMPARE(model.column(), 0);
    QVERIFY(model.focusX() > 0 && model.focusX() < 0.1);
  }

  void shiftAppliesToOneCharacter() {
    KeyboardModel model;
    model.setUiLanguage(QStringLiteral("en"));
    model.open();
    model.focusKey(3, 0); // Shift
    QCOMPARE(model.activate().value("action").toString(),
             QStringLiteral("none"));
    QVERIFY(model.shift());
    QVERIFY(focused(model).value("on").toBool());
    model.focusKey(1, 4);
    QCOMPARE(focused(model).value("label").toString(), QStringLiteral("G"));
    auto result = model.activate();
    QCOMPARE(result.value("action").toString(), QStringLiteral("insert"));
    QCOMPARE(result.value("text").toString(), QStringLiteral("G"));
    QVERIFY(!model.shift());
    QCOMPARE(model.activate().value("text").toString(), QStringLiteral("g"));
  }

  void symbolsLayerKeepsTheFocusPositionAndLanguageResetsIt() {
    KeyboardModel model;
    model.setContext(QStringLiteral("address"));
    model.open();
    model.focusKey(4, 1);
    QCOMPARE(focused(model).value("label").toString(), QStringLiteral("123"));
    model.activate();
    QVERIFY(model.symbols());
    QCOMPARE(model.row(), 4);
    QCOMPARE(model.column(), 1);
    QCOMPARE(focused(model).value("label").toString(), QStringLiteral("ABC"));
    // The context row stays; the letter rows become digits.
    QCOMPARE(key(model, 0, 0).value("label").toString(),
             QStringLiteral("https://"));
    QCOMPARE(key(model, 1, 0).value("label").toString(), QStringLiteral("1"));
    QCOMPARE(key(model, 3, 11).value("label").toString(), QStringLiteral("₽"));
    model.focusKey(4, 2);
    model.activate();
    QCOMPARE(model.language(), QStringLiteral("ru"));
    QVERIFY(!model.symbols());
    QCOMPARE(key(model, 1, 0).value("label").toString(), QStringLiteral("й"));
    // The letters key names the alphabet it returns to.
    model.focusKey(4, 1);
    model.activate();
    QCOMPARE(focused(model).value("label").toString(), QStringLiteral("АБВ"));
  }

  void actionKeysReportTheirAction() {
    KeyboardModel model;
    model.open();
    const QStringList expected{"insert", "left", "right", "backspace",
                               "submit"};
    for (int column = 3; column < 8; ++column) {
      model.focusKey(3, column);
      QCOMPARE(model.activate().value("action").toString(),
               expected[column - 3]);
    }
    model.focusKey(3, 3);
    QCOMPARE(model.activate().value("text").toString(), QStringLiteral(" "));
  }

  void contextKeysInsertTheirText() {
    KeyboardModel model;
    model.setContext(QStringLiteral("address"));
    model.open();
    model.focusKey(0, 4);
    QCOMPARE(model.activate().value("text").toString(),
             QStringLiteral(".local"));
  }

  void reopenRestoresTheLanguageAndKeyPerField() {
    {
      KeyboardModel model;
      model.setField(QStringLiteral("browse"));
      model.setContext(QStringLiteral("address"));
      model.open();
      model.focusKey(4, 2);
      model.activate(); // English to Russian
      model.focusKey(3, 7);
      model.close();
    }
    KeyboardModel again;
    again.setField(QStringLiteral("browse"));
    again.setContext(QStringLiteral("address"));
    again.open();
    QCOMPARE(again.language(), QStringLiteral("ru"));
    QCOMPARE(again.row(), 3);
    QCOMPARE(again.column(), 7);
    // Another field keeps its own defaults.
    KeyboardModel other;
    other.setField(QStringLiteral("shortcut-url"));
    other.setContext(QStringLiteral("address"));
    other.open();
    QCOMPARE(other.language(), QStringLiteral("en"));
    QCOMPARE(other.row(), 2);
    QCOMPARE(other.column(), 4);
  }

  void reopenResetsShiftAndTheSymbolsLayer() {
    KeyboardModel model;
    model.setField(QStringLiteral("name"));
    model.open();
    model.focusKey(3, 0);
    model.activate();
    model.focusKey(3, 1);
    model.activate();
    QVERIFY(model.shift() && model.symbols());
    model.close();
    model.open();
    QVERIFY(!model.shift());
    QVERIFY(!model.symbols());
  }
};

QTEST_GUILESS_MAIN(KeyboardModelTest)
#include "keyboard_model_test.moc"
