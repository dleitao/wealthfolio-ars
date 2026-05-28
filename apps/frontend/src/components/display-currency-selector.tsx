import { DisplayCurrency, useDisplayCurrency } from "@/context/display-currency-context";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@wealthfolio/ui/components/ui/dropdown-menu";

import { Button } from "@wealthfolio/ui/components/ui/button";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { cn } from "@/lib/utils";

const OPTIONS: { value: DisplayCurrency; label: string }[] = [
  { value: "ARS", label: "ARS" },
  { value: "USD_OFICIAL", label: "USD Oficial" },
  { value: "USD_MEP", label: "USD MEP" },
  { value: "USD_CCL", label: "USD CCL" },
];

interface DisplayCurrencySelectorProps {
  className?: string;
}

export function DisplayCurrencySelector({ className }: DisplayCurrencySelectorProps) {
  const { displayCurrency, setDisplayCurrency } = useDisplayCurrency();
  const current = OPTIONS.find((o) => o.value === displayCurrency) ?? OPTIONS[0];

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="sm"
          className={cn("h-8 gap-1 px-2 text-xs font-medium", className)}
        >
          <Icons.DollarSign className="size-3.5" />
          {current.label}
          <Icons.ChevronDown className="size-3" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-36">
        {OPTIONS.map((opt) => (
          <DropdownMenuItem
            key={opt.value}
            onClick={() => setDisplayCurrency(opt.value)}
            className={cn("text-sm", opt.value === displayCurrency && "font-semibold")}
          >
            {opt.label}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
