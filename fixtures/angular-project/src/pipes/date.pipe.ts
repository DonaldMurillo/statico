import { Pipe, PipeTransform } from '@angular/core';

@Pipe({
  name: 'dateFormat',
  standalone: true,
})
export class DatePipe implements PipeTransform {
  transform(value: Date | string, format = 'short'): string {
    const date = typeof value === 'string' ? new Date(value) : value;
    if (format === 'long') {
      return date.toLocaleDateString('en-US', {
        weekday: 'long',
        year: 'numeric',
        month: 'long',
        day: 'numeric',
      });
    }
    return date.toLocaleDateString();
  }
}
