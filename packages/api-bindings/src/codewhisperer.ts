import { CodewhispererCustomization as Customization } from "@kiro/proto/fig";
import { sendCodewhispererListCustomizationRequest } from "./requests.js";

const listCustomizations = async () =>
  (await sendCodewhispererListCustomizationRequest({})).customizations;

export { listCustomizations, Customization };
