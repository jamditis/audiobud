import { useTranslation } from "react-i18next";
import {
  permissionsNeedingAction,
  type ManagedPermission,
  type PermissionSnapshot,
} from "@/lib/permission-controller";

interface AccessibilityPermissionsProps {
  permissions: PermissionSnapshot;
  onRequestAccessibility: () => Promise<void>;
  onRequestMicrophone: () => Promise<void>;
}

const AccessibilityPermissions: React.FC<AccessibilityPermissionsProps> = ({
  permissions,
  onRequestAccessibility,
  onRequestMicrophone,
}) => {
  const { t } = useTranslation();
  const recoveryActions = permissionsNeedingAction(permissions);
  if (permissions.error === "check" && recoveryActions.length === 0) {
    recoveryActions.push(
      permissions.platform === "windows" ? "microphone" : "accessibility",
    );
  }

  if (permissions.platform === "other" || recoveryActions.length === 0) {
    return null;
  }

  const requestPermission = async (
    permission: ManagedPermission,
  ): Promise<void> => {
    try {
      if (permission === "microphone") {
        await onRequestMicrophone();
      } else {
        await onRequestAccessibility();
      }
    } catch (error) {
      console.error(`Failed to request ${permission} permission:`, error);
    }
  };

  return (
    <div className="p-4 w-full rounded-lg border border-mid-gray flex flex-col gap-3">
      {permissions.error !== null && (
        <p className="text-sm text-red-400" role="alert">
          {t(
            permissions.error === "check"
              ? "onboarding.permissions.errors.checkFailed"
              : "onboarding.permissions.errors.requestFailed",
          )}
        </p>
      )}
      {recoveryActions.map((permission) => {
        const isMicrophone = permission === "microphone";
        const isWaiting = permissions[permission] === "waiting";
        return (
          <div
            key={permission}
            className="flex justify-between items-center gap-2"
          >
            <div>
              <p className="text-sm font-medium">
                {t(
                  isMicrophone
                    ? "onboarding.permissions.microphone.title"
                    : "onboarding.permissions.accessibility.title",
                )}
              </p>
              <p className="text-sm text-text/60">
                {t(
                  isMicrophone
                    ? "onboarding.permissions.microphone.description"
                    : "accessibility.permissionsDescription",
                )}
              </p>
            </div>
            <button
              onClick={() => requestPermission(permission)}
              disabled={isWaiting}
              className="min-h-10 px-2 py-1 text-sm font-semibold bg-mid-gray/10 border border-mid-gray/80 hover:bg-logo-primary/10 rounded cursor-pointer hover:border-logo-primary disabled:cursor-wait disabled:opacity-60"
            >
              {isWaiting
                ? t("onboarding.permissions.waiting")
                : isMicrophone && permissions.platform === "macos"
                  ? t("onboarding.permissions.grant")
                  : t("accessibility.openSettings")}
            </button>
          </div>
        );
      })}
    </div>
  );
};

export default AccessibilityPermissions;
