export function useZCodeIntl() {
  return { intl: { formatMessage: ({ id }: { id: string }) => (id === "common.close" ? "Close" : id === "common.loading" ? "Loading" : id) } };
}
