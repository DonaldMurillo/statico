import { Injectable } from '@nestjs/common';
import { LoggerService } from './common/logger.service';

@Injectable()
export class AppService {
  constructor(private readonly logger: LoggerService) {}

  getHello(): string {
    this.logger.log('Returning hello message');
    return 'Hello World!';
  }
}
