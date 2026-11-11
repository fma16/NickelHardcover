#include <QLabel>
#include <QPixmap>
#include <QTimer>
#include <QWidgetAction>

#include <NickelHook.h>

#include "annotations/annotationsdialog.h"
#include "cli.h"
#include "files.h"
#include "journal/journaldialog.h"
#include "menucontroller.h"
#include "review/reviewdialog.h"
#include "search/searchdialog.h"
#include "settings.h"
#include "settings/settingsdialog.h"
#include "signin/signindialog.h"
#include "synccontroller.h"

NickelTouchMenu *MenuController::showMenu(const QList<Item> &items, QWidget *anchor, int offset, bool checkable,
                                          bool decorated) {
  NickelTouchMenu *menu = construct_NickelTouchMenu(anchor);
  NickelTouchMenu__showDecoration(menu, decorated);

  QWidget::connect(menu, &QMenu::aboutToHide, menu, &QWidget::deleteLater);

  for (Item item : items) {
    MenuTextItem *menuItem = construct_MenuTextItem(menu, checkable, true);
    MenuTextItem__setText(menuItem, item.text);

    if (item.checked) {
      MenuTextItem__setSelected(menuItem, true);
    }

    QWidgetAction *action = new QWidgetAction(menu);
    action->setDefaultWidget(menuItem);
    action->setData(item.value);
    menu->addAction(action);

    if (item.disabled) {
      nh_log("%s disabled", qPrintable(item.text));
      action->setEnabled(false);
      menuItem->setStyleSheet("color: #666666;");
    } else {
      action->setEnabled(true);
      MenuTextItem__registerForTapGestures(menuItem);
    }

    QObject::connect(action, &QAction::triggered, menu, &QMenu::hide);
    QObject::connect(menuItem, SIGNAL(tapped(bool)), action, SIGNAL(triggered()));

    menu->addSeparator();
  }

  menu->ensurePolished();
  menu->popup(anchor->parentWidget()->mapToGlobal(anchor->geometry().bottomLeft()) + QPoint(0, offset));

  return menu;
}

MenuController::MenuController(int iconHeight, QWidget *parent) : QWidget(parent), iconHeight(iconHeight) {
  icon = construct_TouchLabel(parent);
  TouchLabel__setHitStateEnabled(icon, false);
  setSelected(false);

  QWidget::connect(icon, SIGNAL(tapped(bool)), this, SLOT(showMainMenu()));
}

void MenuController::setSelected(bool selected) {
  icon->setPixmap(QPixmap(selected ? Files::icon_hit : Files::icon).scaledToHeight(iconHeight));
}

enum MenuOption {
  BACK,
  WANT_TO_READ = 1,
  CURRENTLY_READING = 2,
  READ = 3,
  PAUSED = 4,
  DID_NOT_FINISH = 5,
  SYNC_NOW,
  TOGGLE_ENABLED,
  LINK,
  BOOK_STATUS,
  JOURNAL,
  REVIEW,
  ANNOTATIONS,
  SETTINGS,
  SIGNIN,
  READWISE_ENABLED,
  READWISE_SIGNIN
};

void MenuController::showMainMenu() {
  nh_log("MenuController::showMainMenu()");

  setSelected(true);

  SyncController *syncController = SyncController::getInstance();
  QString contentId = syncController->contentId;
  Settings *settings = Settings::getInstance();

  QList<Item> readwiseItems;
  if (settings->isDual()) {
    if (settings->isSignedIn(true)) {
      readwiseItems = {
          {settings->isEnabled(contentId, true) ? "Disable auto-sync (Readwise)" : "Enable auto-sync (Readwise)",
           MenuOption::READWISE_ENABLED, false, syncController->notRealBook}};
    } else {
      readwiseItems = {{"Sign in to Readwise Books", MenuOption::READWISE_SIGNIN}};
    }
  }

  // In dual mode, the Readwise entry follows the Hardcover auto-sync toggle (or the Readwise sync entry).
  QList<Item> items;
  if (settings->isSignedIn()) {
    items = {
        {settings->isDual() && settings->isSignedIn(true) ? "Sync now (both services)" : "Sync now",
         MenuOption::SYNC_NOW, false, !syncController->isEnabled()},
        {(!settings->isEnabled(contentId) || syncController->notRealBook ? QString("Enable auto-sync")
                                                                         : QString("Disable auto-sync")) +
             (settings->isDual() ? " (Hardcover)" : ""),
         MenuOption::TOGGLE_ENABLED, false, syncController->notRealBook},
    };
    items += readwiseItems;
    items += {
        {settings->getLinkedId(contentId).isEmpty() ? "Manually link book" : "Unlink book", MenuOption::LINK},
        {"Update book status", MenuOption::BOOK_STATUS},
        {"Open reading journal", MenuOption::JOURNAL, false, settings->isReadwise()},
        {"Write a review", MenuOption::REVIEW},
        {"All annotations", MenuOption::ANNOTATIONS, false, settings->isReadwise()},
        {"Settings", MenuOption::SETTINGS},
    };
  } else {
    items = {{"Sign in to " + settings->serviceName(), MenuOption::SIGNIN}};
    if (settings->isDual() && settings->isSignedIn(true)) {
      items.append({"Sync now (Readwise Books)", MenuOption::SYNC_NOW, false, !syncController->isEnabled()});
      items += readwiseItems;
      items.append({"Settings", MenuOption::SETTINGS});
    } else {
      items += readwiseItems;
    }
  }

  NickelTouchMenu *menu = showMenu(items, icon, 6);
  QWidget::connect(menu, &QMenu::aboutToHide, icon, [this] { setSelected(false); });
  QWidget::connect(menu, &QMenu::triggered, this, &MenuController::triggered);

  int statusIndex = -1;
  for (int i = 0; i < items.size(); i++) {
    if (items[i].value.toInt() == MenuOption::BOOK_STATUS) {
      statusIndex = i;
    }
  }
  QList<QLabel *> extraField = menu->findChildren<QLabel *>("extraField");
  if (statusIndex >= 0 && extraField.size() > statusIndex && extraField[statusIndex]) {
    extraField[statusIndex]->setPixmap(QPixmap(Files::arrow_right));
  }
}

void MenuController::showStatusMenu(const Messages &message) {
  if (!message.isUserBook()) {
    ConfirmationDialogFactory__showErrorDialog("Hardcover.app", "Unexpected CLI response for <i>getUserBook</i>");
    return;
  }

  int status = message.user_book->status_id;
  nh_log("MenuController::showStatusMenu(%d)", status);

  setSelected(true);

  NickelTouchMenu *menu = showMenu(
      {
          {"Back", MenuOption::BACK, true},
          {"Want to Read", MenuOption::WANT_TO_READ, status == MenuOption::WANT_TO_READ},
          {"Currently Reading", MenuOption::CURRENTLY_READING, status == MenuOption::CURRENTLY_READING},
          {"Read", MenuOption::READ, status == MenuOption::READ},
          {"Paused", MenuOption::PAUSED, status == MenuOption::PAUSED, Settings::getInstance()->isReadwise()},
          {"Did Not Finish", MenuOption::DID_NOT_FINISH, status == MenuOption::DID_NOT_FINISH},
      },
      icon, 6, true);
  NickelTouchMenu__showDecoration(menu, true);

  QWidget::connect(menu, &QMenu::aboutToHide, icon, [this] { setSelected(false); });
  QWidget::connect(menu, &QMenu::triggered, this, &MenuController::triggered);

  QLabel *check = menu->findChild<QLabel *>("check");
  if (check) {
    check->setPixmap(QPixmap(Files::arrow_left));
  }
}

void MenuController::triggered(const QAction *action) {
  int value = action->data().toInt();
  nh_log("MenuController::triggered(%d)", value);

  switch (value) {
  case MenuOption::BACK:
    showMainMenu();
    break;

  case MenuOption::WANT_TO_READ:
  case MenuOption::CURRENTLY_READING:
  case MenuOption::READ:
  case MenuOption::PAUSED:
  case MenuOption::DID_NOT_FINISH: {
    CLI *cli = CLI::setUserBook(value);
    QObject::connect(cli, &CLI::response, this, &MenuController::showStatusMenu);
    break;
  }

  case MenuOption::SYNC_NOW:
    SyncController::getInstance()->manualSync();
    break;

  case MenuOption::TOGGLE_ENABLED: {
    SyncController *ctl = SyncController::getInstance();
    QString contentId = ctl->contentId;

    Settings *settings = Settings::getInstance();
    bool enabled = !settings->isEnabled(contentId);
    settings->setEnabled(contentId, enabled);

    if (!enabled) {
      ctl->clearReadProgress(contentId);
    }

    break;
  }

  case MenuOption::LINK: {
    SyncController *ctl = SyncController::getInstance();

    if (Settings::getInstance()->getLinkedId(ctl->contentId).isEmpty()) {
      SearchDialog::show(ctl->contentId);
    } else {
      Settings::getInstance()->setLinkedId(ctl->contentId, QString());
    }
    break;
  }

  case MenuOption::BOOK_STATUS: {
    QTimer::singleShot(0, this, [this] { icon->setPixmap(QPixmap(Files::loading).scaledToHeight(iconHeight)); });
    CLI *cli = CLI::getUserBook();
    QObject::connect(cli, &CLI::response, this, &MenuController::showStatusMenu);
    break;
  }

  case MenuOption::JOURNAL:
    JournalDialog::show();
    break;

  case MenuOption::REVIEW:
    ReviewDialog::show();
    break;

  case MenuOption::ANNOTATIONS:
    AnnotationsDialog::show();
    break;

  case MenuOption::SETTINGS:
    SettingsDialog::show();
    break;

  case MenuOption::SIGNIN:
    SignInDialog::show();
    break;

  case MenuOption::READWISE_SIGNIN:
    SignInDialog::show(true);
    break;

  case MenuOption::READWISE_ENABLED: {
    SyncController *ctl = SyncController::getInstance();
    Settings *settings = Settings::getInstance();
    bool enabled = !settings->isEnabled(ctl->contentId, true);
    settings->setEnabled(ctl->contentId, enabled, true);
    if (!enabled)
      ctl->clearReadwiseProgress(ctl->contentId);
    break;
  }
  }
}
